from abc import ABC, abstractmethod

from ..schema import Graph


class Sink(ABC):
    """Writes a Graph somewhere."""

    @abstractmethod
    def write(self, graph: Graph) -> None: ...

    @abstractmethod
    def clone(self) -> "Sink": ...


class CountingSink(Sink):
    def write(self, graph: Graph) -> int:
        return len(graph.nodes)

    def clone(self) -> "CountingSink":
        return CountingSink()


class Mixin:
    def close(self) -> None:
        pass

    def flush(self):
        pass
