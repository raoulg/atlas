from pathlib import Path

from files.types import PathLike


def newest(paths: list[Path]) -> PathLike:
    return max(paths, key=lambda p: p.stat().st_mtime)
