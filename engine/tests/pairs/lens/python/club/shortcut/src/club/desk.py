"""The club's desk, in SQLite: a table per thing it keeps."""

import sqlite3

from club.model import Member, Report, Visit


class Desk:
    """Logs visits and reads them back as the rows they are kept as."""

    def __init__(self, con: sqlite3.Connection) -> None:
        self.con = con

    def log(self, visit: Visit) -> None:
        self.con.execute("INSERT INTO visits (member, day) VALUES (?, ?)", (visit.member, visit.day))

    def visits(self) -> list[tuple[str, str]]:
        return self.con.execute("SELECT member, day FROM visits").fetchall()


class Visits:
    """Puts visits and gets them back; `come` puts a Member's name into a visit."""

    def __init__(self, con: sqlite3.Connection) -> None:
        self.con = con

    def log(self, visit: Visit) -> None:
        self.con.execute("INSERT INTO visits (member, day) VALUES (?, ?)", (visit.member, visit.day))

    def visits(self) -> list[Visit]:
        return [Visit(m, d) for m, d in self.con.execute("SELECT member, day FROM visits").fetchall()]

    def come(self, who: Member, day: str) -> None:
        self.con.execute("INSERT INTO visits (member, day) VALUES (?, ?)", (who.name, day))


class Register:
    """Puts members into `members` and reads back their names; a Member comes back only inside a
    Report, from `reports`."""

    def __init__(self, con: sqlite3.Connection) -> None:
        self.con = con

    def join(self, who: Member) -> None:
        self.con.execute("INSERT INTO members (name) VALUES (?)", (who.name,))

    def names(self) -> list[str]:
        return [r[0] for r in self.con.execute("SELECT name FROM members").fetchall()]

    def latest(self) -> Report:
        title, author = self.con.execute("SELECT title, author FROM reports").fetchone()
        return Report(title, Member(author))
