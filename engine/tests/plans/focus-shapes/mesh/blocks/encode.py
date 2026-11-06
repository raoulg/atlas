"""Blocks of a text model, each saying the shape of what it takes and gives: four ranks of one tensor type."""

from jaxtyping import Float
from torch import Tensor

from mesh.loader.read import Vocab


def tokens(vocab: Vocab, text: str) -> Float[Tensor, "batch seq"]:
    raise NotImplementedError


def embed(ids: Float[Tensor, "batch seq"]) -> Float[Tensor, "batch seq dim"]:
    return ids


def pool(x: Float[Tensor, "batch seq dim"]) -> Float[Tensor, "batch dim"]:
    return x


def widen(x: Float[Tensor, "... dim"]) -> Float[Tensor, "... width"]:
    return x


def spread(x: Float[Tensor, "... width"]) -> Float[Tensor, "..."]:
    return x


def lift(x: Float[Tensor, "batch seq dim"]) -> Float[Tensor, "a b c d"]:
    return x
