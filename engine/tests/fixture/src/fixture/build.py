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
