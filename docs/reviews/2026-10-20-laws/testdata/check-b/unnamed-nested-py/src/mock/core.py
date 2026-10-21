from dataclasses import dataclass, field


@dataclass
class Graph:
    nodes: list[str] = field(default_factory=list)


def load(p: str) -> Graph:
    return Graph([p])
def infer(p: str) -> Graph:
    return Graph([p, p])
def enrich(g: Graph) -> int:
    return len(g.nodes)


def nested(p: str, a: bool, b: bool) -> int:
    if a:
        g = load(p)
    elif b:
        g = infer(p)
    else:
        g = load(p) if p else infer(p)
    return enrich(g)


def main(p: str) -> int:
    return nested(p, True, False)
