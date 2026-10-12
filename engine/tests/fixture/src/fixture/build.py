from pathlib import Path
from typing import Callable, Generic, Type, TypeVar, Union

import pandas as pd
import requests
from huggingface_hub import HfApi, snapshot_download
from jaxtyping import Float
from torch import Tensor

from fixture import DuckSink, Graph
from fixture.schema import Node
from fixture.sinks.base import Sink
from fixture.store import reader

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


def seeded(text: str, seed: bool) -> None:
    g = Graph(nodes=[Node(id=text)]) if seed else Graph(nodes=[])
    enrich(g, Node(id="end"))


def seed_graph(text: str, seed: bool) -> Graph:
    return Graph(nodes=[Node(id=text)]) if seed else Graph(nodes=[])


def seeded_by_name(text: str, seed: bool) -> None:
    enrich(seed_graph(text, seed), Node(id="end"))


SEEN: dict[str, str] = {}


def remember(name: str) -> None:
    SEEN[name] = name


def check_name(name: str) -> None:
    if not name:
        raise ValueError("a name")


def each(names: list[str], fn: Callable[[str], None]) -> None:
    for n in names:
        fn(n)


def register_names(names: list[str]) -> None:
    for n in names:
        check_name(n)
    each(names, remember)


def reread() -> int:
    r = reader()
    return len(r.nodes())


class Pipeline:
    def __init__(self, sink: DuckSink):
        self.sink = sink

    def run(self, url: str) -> None:
        g = build_graph(download(url))
        self.sink.write(g)


def save(frame: pd.DataFrame, name: str, path: Path) -> None:
    frame.to_csv(f"{name}.csv", sep=";")
    frame.to_parquet(path)
    with open(name, "w") as f:
        f.write("done")
    print("saved", name)


READERS = {".csv": pd.read_csv}
Loaded = Union[Graph, Node]


def load(path: Path) -> Loaded:
    return READERS[path.suffix](path)


def register(kind: Type[SinkT]) -> None:
    pass


def describe(graph: "Graph") -> "Node":
    return Node(id=str(len(graph.nodes)))


def pool(x: Float[Tensor, "batch seq dim"]) -> Float[Tensor, "batch dim"]:
    return x.mean(1)


def fetch(repo_id: str) -> str:
    return snapshot_download(repo_id, repo_type="dataset")


def publish(folder: str, repo_id: str) -> None:
    api = HfApi()
    api.upload_folder(repo_id=repo_id, folder_path=folder)


T = TypeVar("T")


class Holder(Generic[T]):
    def __init__(self, item: T) -> None:
        self.item = item


def hold(node: Node) -> "Holder[Node]":
    return Holder(node)


def unwrap(held: Holder[Node]) -> Node:
    return held.item


class Encoder:
    def encode(self, x: Float[Tensor, "batch seq dim"]) -> Float[Tensor, "..."]:
        return x


class MeanEncoder(Encoder):
    def encode(self, x: Float[Tensor, "batch seq dim"]) -> Float[Tensor, "batch dim"]:
        return x.mean(1)


class PatchEncoder(MeanEncoder):
    def encode(self, x: Float[Tensor, "batch seq dim"]) -> Float[Tensor, "batch seq dim"]:
        return x


class Reader:
    def read(self) -> Loaded:
        return Graph()


class GraphReader(Reader):
    def read(self) -> Graph:
        return Graph()
