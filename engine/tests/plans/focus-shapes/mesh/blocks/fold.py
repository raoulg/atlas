"""A rank only the blocks use: made in one module of them and taken in the other."""

from jaxtyping import Float
from torch import Tensor


def squeeze(x: Float[Tensor, "a b c d"]) -> Float[Tensor, "a b c"]:
    return x
