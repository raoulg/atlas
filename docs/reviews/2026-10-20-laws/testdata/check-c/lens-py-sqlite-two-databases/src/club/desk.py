import sqlite3
from dataclasses import dataclass
from pathlib import Path


@dataclass
class Visit:
    member: str
    day: str


class Desk:
    """Appends visits to an audit database; reads members from another database file."""

    def __init__(self, audit: Path, register: Path) -> None:
        self.audit = sqlite3.connect(audit)
        self.register = sqlite3.connect(register)

    def log(self, visit: Visit) -> None:
        self.audit.execute("INSERT INTO visits (member, day) VALUES (?, ?)", (visit.member, visit.day))

    def members(self) -> list[str]:
        return [r[0] for r in self.register.execute("SELECT name FROM members").fetchall()]
