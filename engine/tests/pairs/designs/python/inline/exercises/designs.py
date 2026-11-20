"""The head of the lesson, built in place, and a run of it once on random input."""

import torch
from torch import nn


def head(width: int, classes: int) -> nn.Module:
    return nn.Sequential(nn.Linear(width, classes))


if __name__ == "__main__":
    print(head(8, 3)(torch.randn(2, 8)))
