"""Join two files; the entry names every place."""

from pathlib import Path


def read_pair(first: Path, second: Path) -> str:
    return first.read_text() + second.read_text()


def join(first: Path, second: Path, destination: Path) -> None:
    destination.write_text(read_pair(first, second))


def run() -> None:
    join(Path("data/first.txt"), Path("data/second.txt"), Path("out/combined.txt"))
