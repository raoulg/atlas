"""`save` writes the file itself and calls `stamp`, which writes it too: one own write, one reached."""

from pathlib import Path

OUT = Path("data/out.txt")


def stamp() -> None:
    OUT.write_text("stamp")


def save() -> None:
    OUT.write_text("body")
    stamp()


def run() -> None:
    save()
