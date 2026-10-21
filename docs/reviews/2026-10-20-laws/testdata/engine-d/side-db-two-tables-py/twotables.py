"""Read one table and write another of the same database: neither table is both."""

import sqlite3


def run() -> None:
    con = sqlite3.connect("data/app.db")
    rows = con.execute("SELECT name FROM people").fetchall()
    con.execute("INSERT INTO greetings VALUES (?)", [", ".join(r[0] for r in rows)])
    con.commit()
