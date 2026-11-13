"""Components of a text model, each saying the shape of what it takes and gives, and four ways to run them."""

from jaxtyping import Float, Int
from torch import Tensor


class Lookup:
    def forward(self, ids: Int[Tensor, "batch seq"]) -> Float[Tensor, "batch seq dim"]:
        return ids.float().unsqueeze(-1)


class MeanPool:
    def forward(self, x: Float[Tensor, "batch seq dim"]) -> Float[Tensor, "batch dim"]:
        return x.mean(1)


class Projection:
    def __init__(self, out: int = 16) -> None:
        self.out = out

    def forward(self, x: Float[Tensor, "... dim"]) -> Float[Tensor, "... {self.out}"]:
        return x


class Concatenate:
    def forward(self, a: Float[Tensor, "batch dim"], b: Float[Tensor, "batch _"]) -> Float[Tensor, "batch ndim"]:
        return a


def annotated(ids: Int[Tensor, "batch seq"]) -> Float[Tensor, "batch ndim"]:
    looked: Float[Tensor, "b s d"] = Lookup().forward(ids)
    pooled = MeanPool().forward(looked)
    return Concatenate().forward(pooled, Projection().forward(pooled))


def rebound(ids: Int[Tensor, "batch seq"]) -> Float[Tensor, "batch ndim"]:
    looked = Lookup().forward(ids)
    if ids.numel():
        looked = Lookup().forward(ids)
    pooled = MeanPool().forward(looked)
    return Concatenate().forward(pooled, Projection().forward(pooled))


def cast(ids: Int[Tensor, "batch seq"]) -> Float[Tensor, "batch ndim"]:
    looked = Lookup().forward(ids)
    floats = looked.float()
    pooled = MeanPool().forward(floats)
    return Concatenate().forward(pooled, Projection().forward(pooled))


def narrowed(ids: Int[Tensor, "batch seq"]) -> Float[Tensor, "batch 16"]:
    return Projection().forward(MeanPool().forward(Lookup().forward(ids)))


def hooked(ids: Int[Tensor, "batch seq"]) -> Float[Tensor, "batch 16"]:
    def hook(y: Float[Tensor, "batch dim"]) -> Float[Tensor, "batch 16"]:
        return Projection().forward(y)

    return MeanPool().forward(Lookup().forward(ids))
