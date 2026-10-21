from dataclasses import dataclass, field


@dataclass
class Graph:
    nodes: list[str] = field(default_factory=list)


def enrich(g: Graph) -> int:
    return len(g.nodes)


def before(p: str, c: bool) -> int:
    g = Graph([p])
    n = enrich(g)
    if c:
        g = Graph([p, p])
    else:
        g = Graph()
    return n


def main(p: str) -> int:
    return before(p, True)
