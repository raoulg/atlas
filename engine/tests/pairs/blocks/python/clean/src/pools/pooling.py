from abc import ABC, abstractmethod

from pools.model import Batch, Summary


class Pool(ABC):
    """Takes a batch of rows and gives one value for each row."""

    @abstractmethod
    def reduce(self, batch: Batch) -> Summary: ...


class MeanPool(Pool):
    def __init__(self, floor: float) -> None:
        self.floor = floor

    def reduce(self, batch: Batch) -> Summary:
        return Summary([max(self.floor, sum(row) / len(row)) for row in batch.rows])


class LastPool(Pool):
    def __init__(self, offset: int) -> None:
        self.offset = offset

    def reduce(self, batch: Batch) -> Summary:
        return Summary([row[-1 - self.offset] for row in batch.rows])


def nothing() -> Summary:
    """The summary of a batch without rows."""
    return Summary([])


def widths(batch: Batch) -> list[int]:
    """How many values each row of a batch holds."""
    return [len(row) for row in batch.rows]
