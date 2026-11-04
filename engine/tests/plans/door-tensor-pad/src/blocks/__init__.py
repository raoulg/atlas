"""A block whose door takes a tensor of a shape its own size names and masks it where it equals the module's pad, a tensor
too: the door's tensor is its caller's, the pad the one value of its kind from beyond."""

from jaxtyping import Bool, Float
from torch import Tensor

PAD = Tensor()


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
