"""One step writes a note; a later step reads it with `with open` and returns it."""

from pathlib import Path

NOTE = Path("data/note.txt")


def save() -> None:
    with open(NOTE, "w") as f:
        f.write("hello")


def load() -> str:
    with open(NOTE) as f:
        return f.read()


def run() -> None:
    save()
    print(load())
