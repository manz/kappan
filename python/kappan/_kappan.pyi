from os import PathLike

import numpy as np
import numpy.typing as npt

class Font:
    """A font at a size in px per em."""

    def __init__(
        self,
        path: str | PathLike[str],
        size: float,
        index: int = 0,
        variations: dict[str, float] | None = None,
        hinting: str = "none",
    ) -> None: ...
    def length(self, text: str) -> float:
        """The line's advance in px."""
    def draw(self, canvas: npt.NDArray[np.uint8], x: float, baseline: float, text: str) -> None:
        """Ink `text` into `canvas` (height, width uint8) from `x` on `baseline`; overlaps keep the brighter
        coverage."""
