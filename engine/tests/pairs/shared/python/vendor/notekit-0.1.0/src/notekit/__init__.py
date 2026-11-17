from pathlib import Path

from . import disk


def save(path: str, text: str) -> None:
    """Saves `text` at `path`, and waits until it is on the disk."""
    write(path, text)
    disk.sync(path)


def write(path: str, text: str) -> None:
    """Writes `text` to `path`."""
    Path(path).write_text(text)
