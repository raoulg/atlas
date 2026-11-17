"""Blocks that say the rank of what they take and give, and one that takes a tensor of any shape."""

from jaxtyping import Float
from torch import Tensor


def embed(x: Float[Tensor, "b d"]) -> Float[Tensor, "b s d"]:
    return x


def pool(x: Float[Tensor, "b s d"]) -> Float[Tensor, "b d"]:
    return x


def norm(t: Tensor) -> Tensor:
    return t
