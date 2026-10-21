import sqlite3
from dataclasses import dataclass


@dataclass
class Visit:
    member: str
    day: str


class Desk:
    """Logs visits (write-only) and lists the members (read-only): two tables, no round trip."""

    def __init__(self, con: sqlite3.Connection) -> None:
        self.con = con

    def log(self, visit: Visit) -> None:
        self.con.execute("INSERT INTO visits (member, day) VALUES (?, ?)", (visit.member, visit.day))

    def members(self) -> list[str]:
        return [r[0] for r in self.con.execute("SELECT name FROM members").fetchall()]
