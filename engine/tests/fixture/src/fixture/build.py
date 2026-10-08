from typing import Union

import pandas as pd
import requests

from fixture import Graph
from fixture.schema import Node
from fixture.sinks import DuckSink


def download(url: str) -> str:
    return requests.get(url).text


def build_graph(text: str) -> Graph:
    g = Graph()
    g.add(Node(id=text))
    return g


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
