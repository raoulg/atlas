from dataclasses import dataclass, field


@dataclass
class Node:
    status: str = ""


@dataclass
class Graph:
    nodes: list[Node] = field(default_factory=list)


def stamp(graph: Graph, status: str) -> None:
    for node in graph.nodes:
        node.status = status


def stamp_counted(graph: Graph, status: str) -> int:
    for node in graph.nodes:
        node.status = status
    return len(graph.nodes)


def main() -> int:
    g = Graph(nodes=[Node()])
    stamp(g, "seen")
    return stamp_counted(g, "seen")
