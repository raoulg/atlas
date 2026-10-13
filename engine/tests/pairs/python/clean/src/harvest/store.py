"""Readings kept in SQLite, one row per station and time."""

import sqlite3
from datetime import datetime
from pathlib import Path

from harvest.model import Reading, Unit

SCHEMA = """
CREATE TABLE IF NOT EXISTS readings (
    station TEXT NOT NULL,
    taken_at TEXT NOT NULL,
    temperature REAL NOT NULL,
    humidity REAL NOT NULL,
    unit TEXT NOT NULL,
    PRIMARY KEY (station, taken_at)
)
"""


class ReadingStore:
    """Puts readings into the database at `path` and gets them back."""

    def __init__(self, path: Path) -> None:
        self.path = path

    def connect(self) -> sqlite3.Connection:
        con = sqlite3.connect(self.path)
        con.execute(SCHEMA)
        return con

    def save(self, reading: Reading) -> None:
        with self.connect() as con:
            con.execute(
                "INSERT OR REPLACE INTO readings (station, taken_at, temperature, humidity, unit) VALUES (?, ?, ?, ?, ?)",
                (reading.station, reading.taken_at.isoformat(), reading.temperature, reading.humidity, reading.unit.value),
            )

    def history(self, station: str) -> list[Reading]:
        with self.connect() as con:
            rows = con.execute(
                "SELECT station, taken_at, temperature, humidity, unit FROM readings WHERE station = ? ORDER BY taken_at",
                (station,),
            ).fetchall()
        return [Reading(s, datetime.fromisoformat(t), temperature, humidity, Unit(u)) for s, t, temperature, humidity, u in rows]
