"""Rows kept in a DuckDB file: the directory is made by a function of its own, apart from the
connection that opens the file in it."""

from pathlib import Path

import duckdb

DATABASE = "warehouse/rows.duckdb"


def prepare() -> None:
    Path(DATABASE).parent.mkdir(parents=True, exist_ok=True)


def connect() -> duckdb.DuckDBPyConnection:
    return duckdb.connect(DATABASE)


def count(db: duckdb.DuckDBPyConnection) -> int:
    return db.execute("SELECT count(*) FROM rows").fetchall()[0][0]


def add(db: duckdb.DuckDBPyConnection, name: str) -> None:
    db.execute("INSERT INTO rows VALUES (?)", [name])
