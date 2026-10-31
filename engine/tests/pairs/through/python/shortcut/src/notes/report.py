from pathlib import Path

from notes.sections import Section


def report(section: Section, source: Path) -> None:
    heading = source.read_text()
    Section.show(section, heading)
