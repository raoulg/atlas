import copy
from dataclasses import dataclass, field


@dataclass
class Graph:
    name: str = ""
    nodes: list[str] = field(default_factory=list)


def renamed(graph: Graph, name: str) -> Graph:
    """Hands back a renamed copy; the graph it is handed stays as it was."""
    graph = copy.copy(graph)
    graph.name = name
    return graph


def main() -> Graph:
    return renamed(Graph(), "g")
