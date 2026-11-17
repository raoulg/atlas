"""The data: it hands the blocks a batch and a vocabulary, and takes back what they make, each by its own names for the
axes."""

from jaxtyping import Float
from torch import Tensor


class Vocab:
    def __init__(self, size: int) -> None:
        self.size = size


def vocab() -> Vocab:
    return Vocab(64)


def batches() -> Float[Tensor, "rows cols"]:
    raise NotImplementedError


def score(z: Float[Tensor, "b d"]) -> float:
    return 0.0


def keep(x: Float[Tensor, "b s d"]) -> None:
    pass
