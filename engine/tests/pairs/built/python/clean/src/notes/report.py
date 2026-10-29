from pathlib import Path

from notes.sections import Block, Section


def report(section: Section, source: Path) -> None:
    heading = source.read_text()
    section.show(heading)


def note(block: Block, source: Path) -> None:
    heading = source.read_text()
    block.show(heading)
