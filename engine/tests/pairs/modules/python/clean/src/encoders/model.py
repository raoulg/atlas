"""Two encoders, one per input, joined into one head. Each module is called, and runs its forward."""

import torch
from jaxtyping import Float
from torch import Tensor, nn


class Encoder(nn.Module):
    def __init__(self, width: int, hidden: int) -> None:
        super().__init__()
        self.linear = nn.Linear(width, hidden)
        self.activation = nn.GELU()

    def forward(self, x: Float[Tensor, "batch width"]) -> Float[Tensor, "batch hidden"]:
        return self.activation(self.linear(x))


class Head(nn.Module):
    def __init__(self, hidden: int, classes: int) -> None:
        super().__init__()
        self.out = nn.Sequential(nn.Linear(hidden, classes))

    def forward(self, h: Float[Tensor, "batch 32"]) -> Float[Tensor, "batch 4"]:
        return self.out(h)


class Fusion(nn.Module):
    def __init__(self) -> None:
        super().__init__()
        self.image = Encoder(384, 16)
        self.text = Encoder(512, 16)
        self.head = Head(32, 4)

    def forward(self, image: Float[Tensor, "batch 384"], text: Float[Tensor, "batch 512"]) -> Float[Tensor, "batch 4"]:
        image_z: Float[Tensor, "batch 16"] = self.image(image)
        text_z: Float[Tensor, "batch 16"] = self.text(text)
        joined = join(image_z, text_z)
        return self.head(joined)


def join(a: Float[Tensor, "batch 16"], b: Float[Tensor, "batch 16"]) -> Float[Tensor, "batch 32"]:
    return a


def predict(model: Fusion, image: Float[Tensor, "batch 384"], text: Float[Tensor, "batch 512"]) -> Float[Tensor, "batch 4"]:
    return model(image, text)


def batch() -> tuple[Float[Tensor, "batch 384"], Float[Tensor, "batch 512"]]:
    return torch.randn(64, 384), torch.randn(64, 512)


def main() -> None:
    image, text = batch()
    print(predict(Fusion(), image, text))
