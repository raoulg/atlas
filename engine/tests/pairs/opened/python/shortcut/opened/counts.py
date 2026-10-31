"""Counts kept per kind, each in a database file of its own in the folder `data`, its path glued
together as text."""

import sqlite3


def counts(kind: str) -> sqlite3.Connection:
    return sqlite3.connect("data/" + kind + "/counts.db")


def both(kind: str) -> tuple[sqlite3.Connection, sqlite3.Connection]:
    return sqlite3.connect("data/" + kind + "/counts.db"), sqlite3.connect("data/" + "totals.db")
