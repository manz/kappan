from pathlib import Path

import numpy as np
import pytest

from kappan import Font

NOTO = Path(__file__).parent / "fonts" / "NotoSans-Regular.ttf"


def ink(font: Font, text: str, width: int = 96, height: int = 16) -> np.ndarray:
    canvas = np.zeros((height, width), dtype=np.uint8)
    font.draw(canvas, 2, 12, text)
    return canvas


@pytest.mark.parametrize("hinting", ["none", "light", "full"])
def test_draw_inks_the_canvas(hinting: str) -> None:
    assert ink(Font(NOTO, 11, hinting=hinting), "Café").max() > 200


def test_draw_keeps_the_brighter_coverage() -> None:
    canvas = np.full((16, 96), 200, dtype=np.uint8)
    Font(NOTO, 11).draw(canvas, 2, 12, "Café")
    assert canvas.min() == 200


def test_draw_clips_to_the_canvas() -> None:
    canvas = np.zeros((16, 8), dtype=np.uint8)
    Font(NOTO, 11).draw(canvas, -4, 12, "Mmmm")
    assert canvas.max() > 200


def test_empty_text_inks_nothing() -> None:
    assert ink(Font(NOTO, 11), "").max() == 0


def test_length_includes_kerning() -> None:
    font = Font(NOTO, 11)
    assert font.length("AV") < font.length("A") + font.length("V")


def test_full_hinting_advances_whole_pixels() -> None:
    assert Font(NOTO, 11, hinting="full").length("Une étrange fièvre").is_integer()


def test_light_hinting_keeps_the_linear_width() -> None:
    text = "Une étrange fièvre frappe Laponia."
    assert Font(NOTO, 11, hinting="light").length(text) == Font(NOTO, 11).length(text)


@pytest.mark.parametrize(("hinting", "solid"), [("none", False), ("light", True)])
def test_light_hinting_puts_the_x_height_on_a_pixel_edge(hinting: str, solid: bool) -> None:
    rows = ink(Font(NOTO, 11, hinting=hinting), "zzzz").max(axis=1)
    top = int(np.flatnonzero(rows)[0])
    assert (rows[top] >= 250) == solid


def test_unknown_hinting_is_refused() -> None:
    with pytest.raises(ValueError, match="none, light or full"):
        Font(NOTO, 11, hinting="strong")


def test_unknown_axis_is_refused() -> None:
    with pytest.raises(KeyError, match="wght"):
        Font(NOTO, 11, variations={"wght": 700})


def test_missing_face_is_refused() -> None:
    with pytest.raises(ValueError, match="no font at index 3"):
        Font(NOTO, 11, index=3)


def test_missing_file_is_refused() -> None:
    with pytest.raises(FileNotFoundError):
        Font(NOTO.with_name("nope.ttf"), 11)
