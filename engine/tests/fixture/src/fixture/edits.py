"""What changes in place is what the call does, not what it is called."""

from fixture.schema import Graph, Node

type Part = Graph | Node


class Settings:
    """Its `update` gives a new value; it changes nothing."""

    def __init__(self, depth: int) -> None:
        self.depth = depth

    def update(self, depth: int) -> "Settings":
        return Settings(depth)


def deeper(settings: Settings) -> Settings:
    return settings.update(settings.depth + 1)


def rename(graph: Graph, name: str) -> None:
    setattr(graph, "name", name)


def forget(graph: Graph) -> None:
    del graph.nodes[0]


def first(part: Part) -> str:
    return str(part)
