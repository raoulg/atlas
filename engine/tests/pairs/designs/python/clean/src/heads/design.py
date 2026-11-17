"""A head as a design: each stage a method, a stage the search may leave out named as an option, by its axis as
the first argument or by keyword."""

from dataclasses import dataclass

from jaxtyping import Float
from torch import Tensor, nn
from vectormesh.design import option


@dataclass(frozen=True)
class Config:
    hidden: int = 16
    skip: bool = False
    gate: bool = False


class Block(nn.Module):
    def __init__(self, width: int, hidden: int) -> None:
        super().__init__()
        self.linear = nn.Linear(width, hidden)

    def forward(self, x: Float[Tensor, "batch width"]) -> Float[Tensor, "batch hidden"]:
        return self.linear(x)


class Head(nn.Module):
    def __init__(self, config: Config, width: int, classes: int) -> None:
        super().__init__()
        self.config = config
        self.width = width
        widths = [width, config.hidden]
        self.block = Block(widths[0], widths[1])
        if config.skip:
            self.projection = nn.Linear(width, config.hidden)
        if config.gate:
            self.gating = nn.Linear(config.hidden, config.hidden)
        self.classifier = nn.Linear(config.hidden, classes)

    def hidden(self, x: Float[Tensor, "batch {self.width}"]) -> Float[Tensor, "batch hidden"]:
        return self.block(x)

    @option("skip")
    def skip(self, x: Float[Tensor, "batch {self.width}"]) -> Float[Tensor, "batch hidden"]:
        return self.projection(x)

    @option(axis="gate")
    def gate(self, h: Float[Tensor, "batch hidden"]) -> Float[Tensor, "batch hidden"]:
        return self.gating(h)

    def classify(self, h: Float[Tensor, "batch hidden"]) -> Float[Tensor, "batch classes"]:
        return self.classifier(h)

    def forward(self, x: Float[Tensor, "batch {self.width}"]) -> Float[Tensor, "batch classes"]:
        h = self.hidden(x)
        if self.config.skip:
            h = h + self.skip(x)
        if self.config.gate:
            h = self.gate(h)
        return self.classify(h)
