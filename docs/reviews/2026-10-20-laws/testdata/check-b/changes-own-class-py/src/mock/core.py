from dataclasses import dataclass, field


@dataclass
class Graph:
    name: str = ""
    nodes: list[str] = field(default_factory=list)

    def add(self, n: str) -> None:
        self.nodes.append(n)


def by_method(g: Graph, n: str) -> int:
    g.add(n)
    return 1


def by_attr(g: Graph) -> int:
    g.name = "x"
    return 1


def by_item(g: Graph) -> int:
    g.nodes[0] = "x"
    return 1


def main() -> int:
    g = Graph()
    return by_method(g, "a") + by_attr(g) + by_item(g)
