"""The club's desk, in SQLite: a table per thing it keeps."""

import sqlite3

from club.model import Member, Report, Visit


class Desk:
    """Logs visits and lists the members' names: it only writes `visits` and only reads `members`."""

    def __init__(self, con: sqlite3.Connection) -> None:
        self.con = con

    def log(self, visit: Visit) -> None:
        self.con.execute("INSERT INTO visits (member, day) VALUES (?, ?)", (visit.member, visit.day))

    def members(self) -> list[str]:
        return [r[0] for r in self.con.execute("SELECT name FROM members").fetchall()]


class Visits:
    """Puts visits and gets them back; `forget` names the rows it deletes by their Member."""

    def __init__(self, con: sqlite3.Connection) -> None:
        self.con = con

    def log(self, visit: Visit) -> None:
        self.con.execute("INSERT INTO visits (member, day) VALUES (?, ?)", (visit.member, visit.day))

    def visits(self) -> list[Visit]:
        return [Visit(m, d) for m, d in self.con.execute("SELECT member, day FROM visits").fetchall()]

    def forget(self, who: Member) -> None:
        self.con.execute("DELETE FROM visits WHERE member = ?", (who.name,))


class Register:
    """Puts members and gets them back from `members`; gets reports, which name a Member, from `reports`."""

    def __init__(self, con: sqlite3.Connection) -> None:
        self.con = con

    def join(self, who: Member) -> None:
        self.con.execute("INSERT INTO members (name) VALUES (?)", (who.name,))

    def member(self, name: str) -> Member:
        (found,) = self.con.execute("SELECT name FROM members WHERE name = ?", (name,)).fetchone()
        return Member(found)

    def latest(self) -> Report:
        title, author = self.con.execute("SELECT title, author FROM reports").fetchone()
        return Report(title, Member(author))
