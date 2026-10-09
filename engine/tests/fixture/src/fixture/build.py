from typing import Generic, Type, TypeVar, Union

import pandas as pd
import requests
from huggingface_hub import HfApi, snapshot_download
from jaxtyping import Float
from torch import Tensor

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
