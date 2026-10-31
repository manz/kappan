# kappan

TrueType text as 0-255 coverage for small bitmap screens (SNES translations and the like).
[HarfRust](https://github.com/harfbuzz/harfrust) shapes the line, kerning included,
[Skrifa](https://github.com/googlefonts/fontations) draws each glyph at the size, hinted or not, and
[zeno](https://github.com/dfrg/zeno) fills it into a NumPy canvas. No FreeType, no Pillow.

```python
import numpy as np
from kappan import Font

font = Font("NotoSans-Medium.ttf", 11, hinting="full")
canvas = np.zeros((16, 208), dtype=np.uint8)
font.draw(canvas, 4, 13, "Capitaine, un peu de café ?")
font.length("Capitaine, un peu de café ?")  # px
```

- `hinting`: `"none"` (the outline as designed), `"light"` (vertical only: crisp x-height and baseline, widths
  kept), `"full"` (both axes: each glyph on a whole pixel, advancing by its hinted width plus the kerning).
- `index` picks the face in a collection (`.ttc`), `variations` sets axes by tag (`{"opsz": 17, "wdth": 95}`).
- `draw` keeps the brighter of the canvas and the glyph coverage where they overlap.

`tests/fonts/NotoSans-Regular.ttf` is under the SIL Open Font License (`tests/fonts/OFL.txt`).
