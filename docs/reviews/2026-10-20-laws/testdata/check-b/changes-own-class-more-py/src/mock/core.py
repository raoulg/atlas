from dataclasses import dataclass, field


@dataclass
class Graph:
    nodes: list[str] = field(default_factory=list)


def by_augassign(g: Graph) -> int:
    g.nodes += ["x"]
    return 1


def by_del(g: Graph) -> int:
    del g.nodes[0]
    return 1


def by_setattr(g: Graph) -> int:
    setattr(g, "nodes", [])
    return 1


def main() -> int:
    g = Graph()
    return by_augassign(g) + by_del(g) + by_setattr(g)
