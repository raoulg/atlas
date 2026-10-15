"""Join two files, and read and rename the places a graph holds; the operation obtains its paths
from the environment."""

import os
from pathlib import Path

from neo4j import Driver


def read_pair(first: Path, second: Path) -> str:
    return first.read_text() + second.read_text()


def join() -> None:
    first = Path(os.environ["FIRST"])
    second = Path(os.environ["SECOND"])
    destination = Path(os.environ["DESTINATION"])
    destination.write_text(read_pair(first, second))


def run() -> None:
    join()


def overwrite(path: Path) -> None:
    path.write_text(path.read_text().upper())


def names(db: Driver) -> list[str]:
    with db.session() as session:
        return [record.data()["name"] for record in session.run("MATCH (p:Place) RETURN p.name AS name")]


def rename(db: Driver, old: str, new: str) -> None:
    with db.session() as session:
        session.run("MATCH (p:Place {name: $old}) SET p.name = $new", old=old, new=new)
