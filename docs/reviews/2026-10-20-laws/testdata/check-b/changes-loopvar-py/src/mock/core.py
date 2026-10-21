from dataclasses import dataclass, field


@dataclass
class Node:
    status: str = ""


@dataclass
class Graph:
    nodes: list[Node] = field(default_factory=list)


def stamp(graph: Graph, status: str) -> int:
    """Sets the status of every node of the graph it is handed."""
    for node in graph.nodes:
        node.status = status
    return len(graph.nodes)


def stamp_first(graph: Graph, status: str) -> int:
    graph.nodes[0].status = status
    return 1


def main() -> int:
    g = Graph(nodes=[Node()])
    return stamp(g, "seen") + stamp_first(g, "seen")
