"""Counts kept per kind, each in a database file of its own in the folder `data`, its path joined
part by part."""

import sqlite3
from pathlib import Path


def counts(kind: str) -> sqlite3.Connection:
    return sqlite3.connect(Path("data") / kind / "counts.db")


def both(kind: str) -> tuple[sqlite3.Connection, sqlite3.Connection]:
    return sqlite3.connect(Path("data") / kind / "counts.db"), sqlite3.connect(Path("data") / "totals.db")
