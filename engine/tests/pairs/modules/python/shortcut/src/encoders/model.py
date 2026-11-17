"""Two encoders, one per input, joined into one head. Each module's forward is called by name."""

import torch
from jaxtyping import Float
from torch import Tensor, nn


class Encoder(nn.Module):
    def __init__(self, width: int, hidden: int) -> None:
        super().__init__()
        self.linear = nn.Linear(width, hidden)
        self.activation = nn.GELU()

    def forward(self, x: Float[Tensor, "batch width"]) -> Float[Tensor, "batch hidden"]:
        return self.activation.forward(self.linear.forward(x))


class Head(nn.Module):
    def __init__(self, hidden: int, classes: int) -> None:
        super().__init__()
        self.out = nn.Sequential(nn.Linear(hidden, classes))

    def forward(self, h: Float[Tensor, "batch 32"]) -> Float[Tensor, "batch 4"]:
        return self.out.forward(h)


class Fusion(nn.Module):
    def __init__(self) -> None:
        super().__init__()
        self.image = Encoder(384, 16)
        self.text = Encoder(512, 16)
        self.head = Head(32, 4)

    def forward(self, image: Float[Tensor, "batch 384"], text: Float[Tensor, "batch 512"]) -> Float[Tensor, "batch 4"]:
        image_z: Float[Tensor, "batch 16"] = self.image.forward(image)
        text_z: Float[Tensor, "batch 16"] = self.text.forward(text)
        joined = join(image_z, text_z)
        return self.head.forward(joined)


def join(a: Float[Tensor, "batch 16"], b: Float[Tensor, "batch 16"]) -> Float[Tensor, "batch 32"]:
    return a


def predict(model: Fusion, image: Float[Tensor, "batch 384"], text: Float[Tensor, "batch 512"]) -> Float[Tensor, "batch 4"]:
    return model.forward(image, text)


def batch() -> tuple[Float[Tensor, "batch 384"], Float[Tensor, "batch 512"]]:
    return torch.randn(64, 384), torch.randn(64, 512)


def main() -> None:
    image, text = batch()
    model = Fusion()
    print(predict(model, image, text))
    print(model)


class Projection(nn.Module):
    def __init__(self, width: int, out: int) -> None:
        super().__init__()
        self.out = out
        self.linear = nn.Linear(width, out)

    def forward(self, x: Float[Tensor, "*lead width"]) -> Float[Tensor, "*lead {self.out}"]:
        return self.linear.forward(x)


def branch(projection: Projection, image: Float[Tensor, "batch 384"]) -> Float[Tensor, "batch 16"]:
    return projection.forward(image)


def project() -> None:
    image, _ = batch()
    z = branch(Projection(384, 16), image)
    print(z)


def widened(projection: Projection, image: Float[Tensor, "batch 384"]) -> Float[Tensor, "*lead 16"]:
    return branch(projection, image)


def headed(projection: Projection, head: Head, image: Float[Tensor, "batch 384"]) -> Float[Tensor, "batch 4"]:
    projection.forward(image)
    hidden = projection.forward(image)
    return head.forward(hidden)
