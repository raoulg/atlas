from dataclasses import dataclass
from typing import Optional


@dataclass
class Graph:
    name: str = ""


def optional(graph: Optional[Graph]) -> int:
    if graph is not None:
        graph.name = "x"
    return 1


def forward(graph: "Graph") -> int:
    graph.name = "x"
    return 1


def untyped(graph) -> int:  # noqa: ANN001
    graph.name = "x"
    return 1


def main() -> int:
    g = Graph()
    return optional(g) + forward(g) + untyped(g)
