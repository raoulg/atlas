"""Encoders of a batch of sequences, each saying the shape of what it gives."""

from jaxtyping import Float
from torch import Tensor


class Encoder:
    def encode(self, x: Float[Tensor, "batch seq dim"]) -> Float[Tensor, "..."]:
        return x


class MeanEncoder(Encoder):
    def encode(self, x: Float[Tensor, "batch seq dim"]) -> Float[Tensor, "batch dim"]:
        return x.mean(1)


class PatchEncoder(Encoder):
    def encode(self, x: Float[Tensor, "batch seq dim"]) -> Float[Tensor, "batch seq dim"]:
        return x
