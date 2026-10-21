import sqlite3
from dataclasses import dataclass


@dataclass
class Member:
    name: str


@dataclass
class Visit:
    member: str
    day: str


class Desk:
    """A lawful store of visits; `forget` only names the rows to delete, by their Member."""

    def __init__(self, con: sqlite3.Connection) -> None:
        self.con = con

    def log(self, visit: Visit) -> None:
        self.con.execute("INSERT INTO visits (member, day) VALUES (?, ?)", (visit.member, visit.day))

    def visits(self) -> list[Visit]:
        return [Visit(m, d) for m, d in self.con.execute("SELECT member, day FROM visits")]

    def forget(self, who: Member) -> None:
        self.con.execute("DELETE FROM visits WHERE member = ?", (who.name,))
