"""`save` writes the file itself and calls `stamp`, which appends to it: one own write, one reached."""

from pathlib import Path

OUT = Path("data/out.txt")


def stamp() -> None:
    with open(OUT, "a") as f:
        f.write("stamp")


def save() -> None:
    OUT.write_text("body")
    stamp()


def run() -> None:
    save()
