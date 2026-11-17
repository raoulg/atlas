"""Builds the head with every option on and runs it once."""

import torch

from heads.design import Config, Head


def main() -> None:
    model = Head(Config(skip=True), width=8, classes=3)
    print(model(torch.randn(2, 8)))


if __name__ == "__main__":
    main()
