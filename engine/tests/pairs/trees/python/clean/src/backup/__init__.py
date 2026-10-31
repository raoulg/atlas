"""A folder backed up into another, every file in every folder below it, and the size it holds."""

import shutil
from pathlib import Path


def copy_tree(source: Path, target: Path) -> None:
    """Copy every file below `source` to the same place below `target`."""
    target.mkdir(parents=True, exist_ok=True)
    for entry in source.iterdir():
        if entry.is_dir():
            copy_tree(entry, target / entry.name)
        else:
            shutil.copy(entry, target / entry.name)


def size(source: Path) -> int:
    """The bytes of every file below `source`."""
    return sum(len(p.read_bytes()) for p in source.rglob("*") if p.is_file())


def main() -> None:
    """Back `data` up into `backup`, and say how much it holds."""
    copy_tree(Path("data"), Path("backup"))
    print(size(Path("data")))
