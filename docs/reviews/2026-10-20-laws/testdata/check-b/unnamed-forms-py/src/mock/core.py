from dataclasses import dataclass, field


@dataclass
class Graph:
    nodes: list[str] = field(default_factory=list)


def load(p: str) -> Graph:
    return Graph([p])
def infer(t: str) -> Graph:
    return Graph([t, t])
def enrich(g: Graph) -> int:
    return len(g.nodes)


def two_arms(p: str, c: bool) -> int:
    if c:
        g = load(p)
    else:
        g = infer(p)
    return enrich(g)


def ternary(p: str, c: bool) -> int:
    g = load(p) if c else infer(p)
    return enrich(g)

def main(p: str) -> int:
    return two_arms(p, True) + ternary(p, True)
