"""Builds the head with every option on and runs it once."""

import torch
from jaxtyping import Float
from torch import Tensor

from heads.design import Config, Head


def explain(model: Head, x: Float[Tensor, "batch width"], verbose: bool) -> None:
    # a choice of the script's that holds a call of the option's stage is no choice of the option
    if verbose:
        print(model.skip(x))


def main() -> None:
    model = Head(Config(skip=True), width=8, classes=3)
    print(model(torch.randn(2, 8)))
    explain(model, torch.randn(2, 8), verbose=False)


if __name__ == "__main__":
    main()
