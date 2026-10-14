"""Readings kept in SQLite, one row per station and time, with the outages of a harvest and the
summaries of a day."""

import sqlite3
from datetime import datetime
from pathlib import Path

from harvest.model import Day, Harvest, Outage, Reading, Summary, Unit, day_summary

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

OUTAGES = """
CREATE TABLE IF NOT EXISTS outages (
    station TEXT NOT NULL,
    reason TEXT NOT NULL
)
"""

SUMMARIES = """
CREATE TABLE IF NOT EXISTS summaries (
    station TEXT NOT NULL,
    date TEXT NOT NULL,
    low REAL NOT NULL,
    high REAL NOT NULL,
    mean REAL NOT NULL,
    PRIMARY KEY (station, date)
)
"""


class ReadingStore:
    """Puts readings, outages and summaries into the database at `path` and gets them back."""

    def __init__(self, path: Path) -> None:
        self.path = path

    def connect(self) -> sqlite3.Connection:
        con = sqlite3.connect(self.path)
        con.execute(SCHEMA)
        con.execute(OUTAGES)
        con.execute(SUMMARIES)
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

    def save_outage(self, outage: Outage) -> None:
        with self.connect() as con:
            con.execute("INSERT INTO outages (station, reason) VALUES (?, ?)", (outage.station, outage.reason))

    def kept(self, station: str) -> Harvest:
        """What was kept of a station: its readings, and what the service said when it gave none."""
        with self.connect() as con:
            rows = con.execute("SELECT station, reason FROM outages WHERE station = ?", (station,)).fetchall()
        return Harvest(self.history(station), [Outage(s, reason) for s, reason in rows])

    def summary(self, day: Day) -> Summary:
        """The summary of a day: the one kept, else one made from the day's readings and kept."""
        return self.kept_summary(day) or self.keep_summary(day_summary(day, self.history(day.station)))

    def kept_summary(self, day: Day) -> Summary | None:
        with self.connect() as con:
            row = con.execute(
                "SELECT low, high, mean FROM summaries WHERE station = ? AND date = ?",
                (day.station, day.date.isoformat()),
            ).fetchone()
        return None if row is None else Summary(day.station, day.date, *row)

    def keep_summary(self, summary: Summary) -> Summary:
        with self.connect() as con:
            con.execute(
                "INSERT OR REPLACE INTO summaries (station, date, low, high, mean) VALUES (?, ?, ?, ?, ?)",
                (summary.station, summary.date.isoformat(), summary.low, summary.high, summary.mean),
            )
        return summary
