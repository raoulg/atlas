"""Readings kept in SQLite."""

import sqlite3

from harvest.model import Reading


class ReadingStore:
    def __init__(self, path: str) -> None:
        self.path = path

    def connect(self) -> sqlite3.Connection:
        con = sqlite3.connect(self.path)
        con.row_factory = sqlite3.Row
        con.execute("CREATE TABLE IF NOT EXISTS readings (station TEXT, taken_at TEXT, temperature REAL, humidity REAL, unit TEXT)")
        return con

    def save(self, reading: Reading) -> None:
        # imported here: the harvester module imports this one
        from harvest.harvester import remember

        with self.connect() as con:
            con.execute(
                f"INSERT INTO readings VALUES ('{reading.station}', '{reading.taken_at}', {reading.temperature}, {reading.humidity}, '{reading.unit}')"
            )
        remember(reading)

    def history(self, station: str) -> list[dict]:
        with self.connect() as con:
            rows = con.execute(f"SELECT * FROM readings WHERE station = '{station}' ORDER BY taken_at").fetchall()
        return [dict(row) for row in rows]
