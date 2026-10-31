//! TrueType text as 0-255 coverage for small bitmap screens: HarfRust shapes the line (kerning included), Skrifa
//! draws each glyph's outline at the size, hinted or not, and zeno fills the line's path into a NumPy canvas.

use harfrust::{Buffer, ShapeOptions, ShaperFont};
use numpy::{PyReadwriteArray2, PyUntypedArrayMethods};
use pyo3::exceptions::{PyKeyError, PyValueError};
use pyo3::prelude::*;
use skrifa::{DrawMemory, GlyphId, HintOptions, PathSink, RasterTarget, ScalerFont, Tag};
use std::collections::HashMap;
use zeno::{Command, Fill, Mask, Point};

/// Shaped positions in 1/UNITS px, as HarfBuzz's 26.6.
const UNITS: f32 = 64.0;

/// One glyph of a shaped line, in px: the glyph, its offset from the pen, the pen's advance after it and the part
/// of that advance the shaper added (kerning).
struct Placed {
    glyph: GlyphId,
    dx: f32,
    dy: f32,
    advance: f32,
    kern: f32,
}

/// Collects outlines into zeno commands, placed at a pen with y pointing down.
struct Collector {
    commands: Vec<Command>,
    x: f32,
    y: f32,
}

impl Collector {
    fn point(&self, x: f32, y: f32) -> Point {
        Point::new(self.x + x, self.y - y)
    }
}

impl PathSink for Collector {
    fn move_to(&mut self, x: f32, y: f32) {
        let p = self.point(x, y);
        self.commands.push(Command::MoveTo(p));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.point(x, y);
        self.commands.push(Command::LineTo(p));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (c, p) = (self.point(cx0, cy0), self.point(x, y));
        self.commands.push(Command::QuadTo(c, p));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (c0, c1, p) = (self.point(cx0, cy0), self.point(cx1, cy1), self.point(x, y));
        self.commands.push(Command::CurveTo(c0, c1, p));
    }

    fn close(&mut self) {
        self.commands.push(Command::Close);
    }
}

/// How outlines meet the pixel grid.
#[derive(Clone, Copy, PartialEq)]
enum Hinting {
    /// The outline as designed; glyphs keep their fractional positions.
    None,
    /// Vertical only (the font's hints, else the autohinter): crisp x-height and baseline, kerned widths kept.
    Light,
    /// Both axes, as the font's hints ask: each glyph starts on a whole pixel and advances by its hinted width
    /// plus the kerning, rounded.
    Full,
}

impl Hinting {
    fn parse(name: &str) -> PyResult<Self> {
        match name {
            "none" => Ok(Self::None),
            "light" => Ok(Self::Light),
            "full" => Ok(Self::Full),
            _ => Err(PyValueError::new_err(format!(
                "hinting {name:?}: none, light or full"
            ))),
        }
    }

    fn options(self) -> Option<HintOptions> {
        match self {
            Self::None => None,
            Self::Light => Some(
                HintOptions::auto_fallback()
                    .with_target(RasterTarget::Light)
                    .with_preserve_linear_metrics(true),
            ),
            Self::Full => Some(HintOptions::auto_fallback()),
        }
    }
}

/// A font at a size in px per em.
#[pyclass(frozen)]
struct Font {
    font: skrifa::Font,
    size: f32,
    hinting: Hinting,
}

#[pymethods]
impl Font {
    /// `index` picks the face in a collection (.ttc); `variations` sets axes by tag ({"opsz": 17}); `hinting` is
    /// "none", "light" (vertical only, widths kept) or "full".
    #[new]
    #[pyo3(signature = (path, size, index = 0, variations = None, hinting = "none"))]
    fn new(
        path: std::path::PathBuf,
        size: f32,
        index: u32,
        variations: Option<HashMap<String, f32>>,
        hinting: &str,
    ) -> PyResult<Self> {
        let data = std::fs::read(&path)?;
        let font = skrifa::Font::new(data, index).ok_or_else(|| {
            PyValueError::new_err(format!("{}: no font at index {index}", path.display()))
        })?;
        let mut settings = Vec::new();
        for (tag, value) in variations.unwrap_or_default() {
            let tag =
                Tag::new_checked(tag.as_bytes()).map_err(|_| PyKeyError::new_err(tag.clone()))?;
            if font.axes().get_by_tag(tag).is_none() {
                return Err(PyKeyError::new_err(format!(
                    "{}: no {tag} axis",
                    path.display()
                )));
            }
            settings.push((tag, value));
        }
        let font = if settings.is_empty() {
            font
        } else {
            font.instance_builder().variations(settings).build()
        };
        Ok(Self {
            font,
            size,
            hinting: Hinting::parse(hinting)?,
        })
    }

    /// The line's advance in px.
    fn length(&self, text: &str) -> PyResult<f32> {
        let mut sink = Collector {
            commands: Vec::new(),
            x: 0.0,
            y: 0.0,
        };
        self.run(text, &mut sink)
    }

    /// Ink `text` into `canvas` (height, width uint8) from `x` on `baseline`; overlaps keep the brighter coverage.
    fn draw(
        &self,
        mut canvas: PyReadwriteArray2<'_, u8>,
        x: f32,
        baseline: f32,
        text: &str,
    ) -> PyResult<()> {
        let (height, width) = (canvas.shape()[0], canvas.shape()[1]);
        let mut sink = Collector {
            commands: Vec::new(),
            x,
            y: baseline,
        };
        self.run(text, &mut sink)?;
        let mut coverage = vec![0u8; width * height];
        Mask::new(&sink.commands[..])
            .style(Fill::NonZero)
            .size(width as u32, height as u32)
            .render_into(&mut coverage, None);
        let mut canvas = canvas.as_array_mut();
        for ((row, col), value) in canvas.indexed_iter_mut() {
            *value = (*value).max(coverage[row * width + col]);
        }
        Ok(())
    }
}

impl Font {
    /// Draws the line's outlines into `sink` from its pen; returns the line's advance.
    fn run(&self, text: &str, sink: &mut Collector) -> PyResult<f32> {
        let scaler = match self.hinting.options() {
            Some(options) => ScalerFont::hinted(self.font.clone(), self.size, options),
            None => ScalerFont::unhinted(self.font.clone(), Some(self.size)),
        };
        let full = self.hinting == Hinting::Full;
        let start = if full { sink.x.round() } else { sink.x };
        sink.x = start;
        for placed in self.shape(text)? {
            let (pen_x, pen_y) = (sink.x, sink.y);
            sink.x += if full { placed.dx.round() } else { placed.dx };
            sink.y -= placed.dy;
            let drawn = match scaler.outline(placed.glyph) {
                Some(outline) => Some(outline.draw(DrawMemory::Scratch, sink).map_err(|e| {
                    PyValueError::new_err(format!("glyph {}: {e}", placed.glyph.to_u32()))
                })?),
                None => None,
            };
            let advance = match drawn.and_then(|m| m.h_advance) {
                Some(hinted) if full => hinted.round() + placed.kern.round(),
                _ => placed.advance,
            };
            sink.x = pen_x + advance;
            sink.y = pen_y;
        }
        Ok(sink.x - start)
    }

    fn shape(&self, text: &str) -> PyResult<Vec<Placed>> {
        let shaper = ShaperFont::new(&self.font).with_scale((self.size * UNITS) as i32);
        let mut buffer = Buffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        harfrust::shape(
            &shaper,
            &mut buffer,
            ShapeOptions::new().point_size(Some(self.size)),
        )
        .map_err(|e| PyValueError::new_err(format!("{text:?}: {e}")))?;
        Ok(buffer
            .glyph_infos()
            .iter()
            .zip(buffer.glyph_positions())
            .map(|(info, pos)| Placed {
                glyph: GlyphId::new(info.glyph_id),
                dx: pos.x_offset as f32 / UNITS,
                dy: pos.y_offset as f32 / UNITS,
                advance: pos.x_advance as f32 / UNITS,
                kern: (pos.x_advance - shaper.glyph_h_advance(GlyphId::new(info.glyph_id))) as f32
                    / UNITS,
            })
            .collect())
    }
}

#[pymodule]
fn _kappan(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Font>()
}
