"""The study of the lesson: the head trained for a few steps."""

import torch
from torch import Tensor, nn

from exercises.designs import head


def train(model: nn.Module, steps: int) -> Tensor:
    x = torch.randn(4, 8)
    out = model(x)
    for _ in range(steps):
        out = model(x)
    return out


def main() -> None:
    print(train(head(8, 3), steps=3))


if __name__ == "__main__":
    main()
