"""Rows kept in a DuckDB file: the connection makes the directory its file lives in, and every
function that reads or adds rows is handed the connection."""

from pathlib import Path

import duckdb

DATABASE = "warehouse/rows.duckdb"


def connect() -> duckdb.DuckDBPyConnection:
    Path(DATABASE).parent.mkdir(parents=True, exist_ok=True)
    return duckdb.connect(DATABASE)


def count(db: duckdb.DuckDBPyConnection) -> int:
    return db.execute("SELECT count(*) FROM rows").fetchall()[0][0]


def add(db: duckdb.DuckDBPyConnection, name: str) -> None:
    db.execute("INSERT INTO rows VALUES (?)", [name])
