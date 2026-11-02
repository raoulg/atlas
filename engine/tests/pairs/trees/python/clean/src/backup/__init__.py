"""A folder backed up into another, every file in every folder below it, and the size it holds; a second folder
mirrored into a third the same way, an entry at a time."""

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


def mirror(source: Path, target: Path) -> None:
    """Copy every file below `source` to the same place below `target`, an entry at a time."""
    target.mkdir(parents=True, exist_ok=True)
    for entry in source.iterdir():
        mirror_entry(entry, target / entry.name)


def mirror_entry(entry: Path, target: Path) -> None:
    """Copy one entry of a folder to `target`: a file as it is, a folder with all it holds."""
    if entry.is_dir():
        mirror(entry, target)
    else:
        shutil.copy(entry, target)


def size(source: Path) -> int:
    """The bytes of every file below `source`."""
    return sum(len(p.read_bytes()) for p in source.rglob("*") if p.is_file())


def main() -> None:
    """Back `data` up into `backup`, say how much it holds, and mirror `photos` into `archive`."""
    copy_tree(Path("data"), Path("backup"))
    print(size(Path("data")))
    mirror(Path("photos"), Path("archive"))
