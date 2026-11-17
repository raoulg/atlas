"""A module whose values all have one shape: its rank family holds that shape alone."""

from jaxtyping import Float
from torch import Tensor, nn


class Scale(nn.Module):
    def __init__(self, width: int) -> None:
        super().__init__()
        self.linear = nn.Linear(width, width)

    def forward(self, x: Float[Tensor, "batch width"]) -> Float[Tensor, "batch width"]:
        return self.linear(x)
