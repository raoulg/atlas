from typing import Type, TypeVar, Union

import pandas as pd
import requests

from fixture import DuckSink, Graph
from fixture.schema import Node
from fixture.sinks.base import Sink

SinkT = TypeVar("SinkT", bound="Sink")


def download(url: str) -> str:
    return requests.get(url).text


def build_graph(text: str) -> Graph:
    g = Graph()
    g.add(Node(id=text))
    return g


def enrich(graph: Graph, extra: Node) -> None:
    graph.add(extra)


def empty() -> Graph:
    return Graph()


def prepare(text: str, mode: str) -> None:
    g = build_graph(text) if mode == "fresh" else empty()
    enrich(g, Node(id=text))
    if mode == "fresh":
        print("built")


def fresh_or_empty(text: str, fresh: bool) -> Graph:
    return build_graph(text) if fresh else empty()


class Pipeline:
    def __init__(self, sink: DuckSink):
        self.sink = sink

    def run(self, url: str) -> None:
        g = build_graph(download(url))
        self.sink.write(g)


def save(frame, name: str, path) -> None:
    frame.to_csv(f"{name}.csv", sep=";")
    frame.to_parquet(path)
    with open(name, "w") as f:
        f.write("done")
    print("saved", name)


READERS = {".csv": pd.read_csv}
Loaded = Union[Graph, Node]


def load(path) -> Loaded:
    return READERS[path.suffix](path)


def register(kind: Type[SinkT]) -> None:
    pass


def describe(graph: "Graph") -> "Node":
    return Node(id=str(len(graph.nodes)))
