"""A log of readings, read through a connection of its own that says what it is."""

import sqlite3
from pathlib import Path


class Journal:
    def __init__(self, path: Path) -> None:
        self.path = path

    def connection(self) -> sqlite3.Connection:
        return sqlite3.connect(self.path)

    def entries(self) -> int:
        con = self.connection()
        return con.execute("SELECT count(*) FROM log").fetchone()[0]
