"""Join two files; the entry names every place."""

from pathlib import Path


def join(first: Path, second: Path, destination: Path) -> None:
    destination.write_text(first.read_text() + second.read_text())


def run() -> None:
    join(Path("data/first.txt"), Path("data/second.txt"), Path("out/combined.txt"))
