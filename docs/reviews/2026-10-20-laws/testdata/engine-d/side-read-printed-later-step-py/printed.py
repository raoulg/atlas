"""One step writes a note; a later step only prints what it reads there."""

from pathlib import Path

NOTE = Path("data/note.txt")


def save() -> None:
    NOTE.write_text("hello")


def show() -> None:
    print(NOTE.read_text())


def run() -> None:
    save()
    show()
