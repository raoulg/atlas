"""Services a library reaches by its table alone: a SQL server, a key-value store, the database
a frame is read from and written to, a program, a notebook's kernel and a model."""

import asyncio
import os
import sqlite3

import asyncpg
import nbclient
import pandas as pd
import psycopg
import redis
from nbclient import NotebookClient
from nbformat import NotebookNode
from torch import Tensor
from transformers import PreTrainedModel


def latest(dsn: str) -> list:
    with psycopg.connect(dsn) as conn:
        return conn.execute("SELECT * FROM readings").fetchall()


async def total(conn: asyncpg.Connection) -> int:
    return await conn.fetchval("SELECT count(*) FROM readings")


async def record(conn: asyncpg.Connection, value: float) -> None:
    await conn.execute("INSERT INTO readings (value) VALUES ($1)", value)


def cache() -> redis.Redis:
    return redis.Redis(host="localhost")


def remember(r: redis.Redis, token: str) -> None:
    r.set("session", token)


def recall(r: redis.Redis) -> bytes | None:
    return r.get("session")


def frame(con: sqlite3.Connection) -> pd.DataFrame:
    return pd.read_sql("SELECT * FROM readings", con)


def table(con: sqlite3.Connection) -> pd.DataFrame:
    return pd.read_sql_table("readings", con)


def keep(readings: pd.DataFrame, con: sqlite3.Connection) -> None:
    readings.to_sql("readings", con, if_exists="append")


def rebuild() -> int:
    return os.system("make report")


async def convert(path: str) -> int:
    proc = await asyncio.create_subprocess_exec("pandoc", path)
    return await proc.wait()


async def shell(command: str) -> int:
    proc = await asyncio.create_subprocess_shell(command)
    return await proc.wait()


def run_notebook(nb: NotebookNode) -> NotebookNode:
    return NotebookClient(nb).execute()


def run_quick(nb: NotebookNode) -> NotebookNode:
    return nbclient.execute(nb)


def complete(model: PreTrainedModel, ids: Tensor) -> Tensor:
    return model.generate(ids, max_new_tokens=20)
