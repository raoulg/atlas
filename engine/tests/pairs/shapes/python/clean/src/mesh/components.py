"""Components of a text model, each saying the shape of what it takes and gives."""

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


def embed(ids: Int[Tensor, "batch seq"]) -> Float[Tensor, "batch ndim"]:
    pooled = MeanPool().forward(Lookup().forward(ids))
    return Concatenate().forward(pooled, Projection().forward(pooled))
