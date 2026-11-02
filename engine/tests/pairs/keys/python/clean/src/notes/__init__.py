"""Notes kept in a folder, each named by its key, where it lies within the folder, and read back by its key."""

from pathlib import Path


def key(root: Path, note: Path) -> Path:
    """Where `note` lies within `root`."""
    return note.relative_to(root)


def read(root: Path, key: Path) -> str:
    """The note `key` names in `root`."""
    return (root / key).read_text()


def main() -> None:
    """Print every note in `notes`, read back by its key."""
    root = Path("notes")
    for note in root.iterdir():
        print(read(root, key(root, note)))
