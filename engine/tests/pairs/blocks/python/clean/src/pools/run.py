from pools.model import Batch, Summary
from pools.pooling import LastPool, MeanPool, Pool, nothing, widths


def choose(name: str) -> Pool:
    return MeanPool(0.0) if name == "mean" else LastPool(0)


def summarize(rows: list[list[float]], name: str) -> Summary:
    if not rows or min(widths(Batch(rows))) == 0:
        return nothing()
    return choose(name).reduce(Batch(rows))
