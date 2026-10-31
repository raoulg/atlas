"""Components of a text model, each taking and giving a tensor."""

from torch import Tensor


class Lookup:
    def forward(self, ids: Tensor) -> Tensor:
        return ids.float().unsqueeze(-1)


class MeanPool:
    def forward(self, x: Tensor) -> Tensor:
        return x.mean(1)


class Projection:
    def __init__(self, out: int = 16) -> None:
        self.out = out

    def forward(self, x: Tensor) -> Tensor:
        return x


class Concatenate:
    def forward(self, a: Tensor, b: Tensor) -> Tensor:
        return a


def embed(ids: Tensor) -> Tensor:
    pooled = MeanPool().forward(Lookup().forward(ids))
    return Concatenate().forward(pooled, Projection().forward(pooled))
