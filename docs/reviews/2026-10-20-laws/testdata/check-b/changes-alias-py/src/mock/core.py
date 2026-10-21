from dataclasses import dataclass


@dataclass
class Graph:
    name: str = ""


def rename(graph: Graph, name: str) -> int:
    g = graph
    g.name = name
    return len(name)


def main() -> int:
    return rename(Graph(), "g")
