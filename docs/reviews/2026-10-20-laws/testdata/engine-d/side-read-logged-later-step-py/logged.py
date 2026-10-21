"""One step writes a note; a later step only logs what it reads there."""

import logging
from pathlib import Path

NOTE = Path("data/note.txt")


def save() -> None:
    NOTE.write_text("hello")


def show() -> None:
    logging.info(NOTE.read_text())


def run() -> None:
    save()
    show()
