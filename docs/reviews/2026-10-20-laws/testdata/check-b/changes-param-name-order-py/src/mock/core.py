from dataclasses import dataclass


@dataclass
class Graph:
    name: str = ""

    def steal_other(self, other: "Graph") -> int:
        self.name = other.name
        other.name = ""
        return 1

    def steal_victim(self, victim: "Graph") -> int:
        self.name = victim.name
        victim.name = ""
        return 1


def swap(a: Graph, z: Graph) -> int:
    a.name, z.name = z.name, a.name
    return 1


def main() -> int:
    g = Graph()
    return g.steal_other(Graph()) + g.steal_victim(Graph()) + swap(g, Graph())
