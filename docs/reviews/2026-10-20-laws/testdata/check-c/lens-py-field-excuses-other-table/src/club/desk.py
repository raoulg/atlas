import sqlite3
from dataclasses import dataclass


@dataclass
class Member:
    name: str


@dataclass
class Report:
    title: str
    author: Member


class Desk:
    """Puts members into `members`; gets reports from `reports`, whose author field is a Member."""

    def __init__(self, con: sqlite3.Connection) -> None:
        self.con = con

    def join(self, who: Member) -> None:
        self.con.execute("INSERT INTO members (name) VALUES (?)", (who.name,))

    def latest(self) -> Report:
        title, author = self.con.execute("SELECT title, author FROM reports").fetchone()
        return Report(title, Member(author))
