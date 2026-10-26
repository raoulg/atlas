from os import PathLike
from typing import Any

from matplotlib.axes import Axes

class Figure:
    def __init__(
        self, figsize: tuple[float, float] | None = None, dpi: float | None = None, *, layout: str | None = None, **kwargs: Any
    ) -> None: ...
    def add_subplot(self, *args: Any, **kwargs: Any) -> Axes: ...
    def savefig(self, fname: str | PathLike[str], **kwargs: Any) -> None: ...
