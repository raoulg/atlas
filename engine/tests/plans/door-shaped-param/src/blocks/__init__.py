"""A block whose door takes a tensor of a shape its own size names, masks it and keeps the result under its name."""

from jaxtyping import Bool, Float
from torch import Scale, Tensor

PAD = Scale()


class Block:
    def __init__(self, size: int) -> None:
        self.size = size

    @staticmethod
    def _mask(x: Float[Tensor, "batch dim"]) -> Bool[Tensor, "batch"]:
        return x == PAD

    def forward(self, x: Float[Tensor, "batch {self.size}"]) -> Float[Tensor, "batch {self.size}"]:
        mask = self._mask(x)
        x = x * mask
        return x
