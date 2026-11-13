from pools.model import Batch, Summary


class MeanPool:
    def __init__(self, floor: float) -> None:
        self.floor = floor

    def reduce(self, batch: Batch) -> Summary:
        return Summary([max(self.floor, sum(row) / len(row)) for row in batch.rows])


class LastPool:
    def __init__(self, offset: int) -> None:
        self.offset = offset

    def pick(self, batch: Batch) -> Summary:
        return Summary([row[-1 - self.offset] for row in batch.rows])


def nothing() -> Summary:
    """The summary of a batch without rows."""
    return Summary([])
