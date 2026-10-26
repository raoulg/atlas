from os import PathLike
from typing import Any

class DataFrame:
    def to_csv(self, path_or_buf: str | PathLike[str] | None = None, **kwargs: Any) -> None: ...
