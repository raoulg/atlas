import sqlite3
from dataclasses import dataclass


@dataclass
class Visit:
    member: str
    day: str


class Swapped:
    """Typed as a lawful lens; get(put(v)) swaps member and day."""

    def __init__(self, con: sqlite3.Connection) -> None:
        self.con = con

    def log(self, visit: Visit) -> None:
        self.con.execute("INSERT INTO visits (member, day) VALUES (?, ?)", (visit.member, visit.day))

    def visits(self) -> list[Visit]:
        return [Visit(member=d, day=m) for m, d in self.con.execute("SELECT member, day FROM visits")]


class Archive:
    """Puts into `visits`, gets from `archive`: what was put never comes back."""

    def __init__(self, con: sqlite3.Connection) -> None:
        self.con = con

    def log(self, visit: Visit) -> None:
        self.con.execute("INSERT INTO visits (member, day) VALUES (?, ?)", (visit.member, visit.day))

    def visits(self) -> list[Visit]:
        return [Visit(m, d) for m, d in self.con.execute("SELECT member, day FROM archive")]
