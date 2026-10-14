"""Readings kept in SQLite."""

import sqlite3

from harvest.model import Day, Outage, Reading


class ReadingStore:
    def __init__(self, path: str) -> None:
        self.path = path

    def connect(self) -> sqlite3.Connection:
        con = sqlite3.connect(self.path)
        con.row_factory = sqlite3.Row
        con.execute("CREATE TABLE IF NOT EXISTS readings (station TEXT, taken_at TEXT, temperature REAL, humidity REAL, unit TEXT)")
        con.execute("CREATE TABLE IF NOT EXISTS outages (station TEXT, reason TEXT)")
        con.execute("CREATE TABLE IF NOT EXISTS summaries (station TEXT, date TEXT, low REAL, high REAL, mean REAL)")
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

    def save_outage(self, outage: Outage) -> None:
        with self.connect() as con:
            con.execute(f"INSERT INTO outages VALUES ('{outage.station}', '{outage.reason}')")

    def outages(self, station: str) -> list[dict]:
        with self.connect() as con:
            rows = con.execute(f"SELECT * FROM outages WHERE station = '{station}'").fetchall()
        return [dict(row) for row in rows]

    def summary(self, day: Day) -> dict:
        with self.connect() as con:
            query = f"SELECT * FROM summaries WHERE station = '{day.station}' AND date = '{day.date}'"
            row = con.execute(query).fetchone()
            if row is None:
                temperatures = [r["temperature"] for r in self.history(day.station) if r["taken_at"].startswith(day.date)]
                low, high, mean = min(temperatures), max(temperatures), sum(temperatures) / len(temperatures)
                con.execute(f"INSERT INTO summaries VALUES ('{day.station}', '{day.date}', {low}, {high}, {mean})")
                row = con.execute(query).fetchone()
        return dict(row)
