"""The head of the lesson, a module of its own, and a run of it once on random input."""

import torch
from jaxtyping import Float
from torch import Tensor, nn


class Head(nn.Module):
    def __init__(self, width: int, classes: int) -> None:
        super().__init__()
        self.classifier = nn.Linear(width, classes)

    def forward(self, x: Float[Tensor, "batch width"]) -> Float[Tensor, "batch classes"]:
        return self.classifier(x)


if __name__ == "__main__":
    print(Head(8, 3)(torch.randn(2, 8)))
