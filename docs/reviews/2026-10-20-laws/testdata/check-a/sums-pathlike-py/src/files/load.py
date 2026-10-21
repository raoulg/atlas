from pathlib import Path

from files.types import PathLike


def load(path: PathLike) -> str:
    return Path(path).read_text()
