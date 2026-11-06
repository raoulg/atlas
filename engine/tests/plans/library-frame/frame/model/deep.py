"""A rank only the model uses: made in one function of it and taken by another."""

from jaxtyping import Float
from torch import Tensor


def lift(x: Float[Tensor, "b s d"]) -> Float[Tensor, "a b c d"]:
    return x


def squeeze(x: Float[Tensor, "a b c d"]) -> Float[Tensor, "b s d"]:
    return x
