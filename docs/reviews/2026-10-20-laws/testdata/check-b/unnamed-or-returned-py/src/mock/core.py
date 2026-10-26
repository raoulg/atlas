from dataclasses import dataclass, field


@dataclass
class Graph:
    nodes: list[str] = field(default_factory=list)


def cached(t: str) -> Graph | None:
    return None
def infer(t: str) -> Graph:
    return Graph([t, t])
def enrich(g: Graph) -> int:
    return len(g.nodes)


def or_else(p: str) -> int:
    g = cached(p) or infer(p)
    return enrich(g)


def returned(p: str, c: bool) -> Graph:
    g = infer(p) if c else Graph()
    return g


def main(p: str) -> int:
    return or_else(p) + len(returned(p, True).nodes)
