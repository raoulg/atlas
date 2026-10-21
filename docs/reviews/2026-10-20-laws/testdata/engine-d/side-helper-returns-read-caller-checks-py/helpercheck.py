"""A helper returns what it reads; the route only checks it after writing the file."""

from pathlib import Path

NOTE = Path("data/note.txt")


def save() -> None:
    NOTE.write_text("hello")


def load() -> str:
    return NOTE.read_text()


def run() -> None:
    save()
    if load() != "hello":
        raise RuntimeError("lost")
