"""Join two files, and read and rename the places a graph holds; the entry names every place, and
each call to the graph says which way its data goes."""

from pathlib import Path

from neo4j import Driver, ManagedTransaction


def read_pair(first: Path, second: Path) -> str:
    return first.read_text() + second.read_text()


def join(first: Path, second: Path, destination: Path) -> None:
    destination.write_text(read_pair(first, second))


def run() -> None:
    join(Path("data/first.txt"), Path("data/second.txt"), Path("out/combined.txt"))


def overwrite(path: Path) -> None:
    path.write_text(path.read_text().upper())


def _names(tx: ManagedTransaction) -> list[str]:
    return [record.data()["name"] for record in tx.run("MATCH (p:Place) RETURN p.name AS name")]


def _rename(tx: ManagedTransaction, old: str, new: str) -> None:
    tx.run("MATCH (p:Place {name: $old}) SET p.name = $new", old=old, new=new)


def names(db: Driver) -> list[str]:
    with db.session() as session:
        return session.execute_read(_names)


def rename(db: Driver, old: str, new: str) -> None:
    with db.session() as session:
        session.execute_write(_rename, old, new)
