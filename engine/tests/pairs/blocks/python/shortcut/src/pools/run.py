from pools.model import Batch, Summary
from pools.pooling import LastPool, MeanPool, nothing, widths


def summarize(rows: list[list[float]], name: str) -> Summary:
    if not rows or min(widths(Batch(rows))) == 0:
        return nothing()
    if name == "mean":
        return MeanPool(0.0).reduce(Batch(rows))
    return LastPool(0).pick(Batch(rows))
