from abc import ABC, abstractmethod

from ..schema import Graph


class Sink(ABC):
    """Writes a Graph somewhere."""

    @abstractmethod
    def write(self, graph: Graph) -> None: ...


class Mixin:
    def close(self) -> None:
        pass
