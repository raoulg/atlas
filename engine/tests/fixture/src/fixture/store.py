"""A store and its readers, found by the types of what holds them."""

import zipfile

import requests
from neo4j import Driver, GraphDatabase

from fixture.schema import Graph


def _session() -> requests.Session:
    return requests.Session()


SESSION = _session()


def pull(url: str) -> dict:
    return SESSION.get(url).json()


def unpack(archive: str, into: str) -> None:
    with zipfile.ZipFile(archive) as zf:
        zf.extractall(into)


class Connection:
    def __init__(self, uri: str):
        self.uri = uri
        self._driver: Driver | None = None

    @property
    def driver(self) -> Driver:
        if self._driver is None:
            self._driver = GraphDatabase.driver(self.uri)
        return self._driver

    def run(self, query: str) -> list:
        with self.driver.session() as session:
            return list(session.run(query))


class Store:
    def __init__(self, uri: str = "bolt://localhost", connection: Connection | None = None):
        self.conn = connection or Connection(uri)

    def write(self, graph: Graph) -> None:
        conn = self.conn
        conn.run(f"CREATE (:Graph {{n: {len(graph.nodes)}}})")


class Reader:
    def __init__(self, store: Store):
        self.store = store

    def nodes(self) -> list:
        return self.store.conn.run("MATCH (n) RETURN n")


class Copier(Reader):
    def copy(self, graph: Graph) -> None:
        self.store.write(graph)


def stores() -> tuple[Store, Reader]:
    store = Store()
    return store, Reader(store)


def keep(graph: Graph) -> list:
    store, reader = stores()
    store.write(graph)
    return reader.nodes()


def reader() -> Reader:
    return Reader(Store())


current = reader


def latest() -> list:
    return current().nodes()


def firsts(graphs: list[Graph]) -> list:
    return [g.first() for g in graphs]


def merged(graphs: list[Graph]) -> Graph:
    out = Graph()
    for g in graphs:
        out.add(g.first())
    return out
