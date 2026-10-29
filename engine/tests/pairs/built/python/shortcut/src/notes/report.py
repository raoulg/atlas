from pathlib import Path

from notes.sections import Block, Section


def report(source: Path) -> None:
    heading = source.read_text()
    Section().show(heading)


def note(source: Path) -> None:
    heading = source.read_text()
    Block().show(heading)
