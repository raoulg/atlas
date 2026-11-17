"""The layers a stack is built of."""

from dataclasses import dataclass


@dataclass(frozen=True)
class Config:
    widths: list[int]
    activation: str
    dropout: float


class Layer:
    """One layer of a stack."""


class Linear(Layer):
    def __init__(self, width_in: int, width_out: int) -> None:
        self.width_in = width_in
        self.width_out = width_out


class Dropout(Layer):
    def __init__(self, p: float) -> None:
        self.p = p


class GELU(Layer):
    pass


class ReLU(Layer):
    pass


class Tanh(Layer):
    pass


class Readout(Layer):
    def __init__(self, config: Config) -> None:
        self.width = config.widths[-1]


class Calibrate(Layer):
    def __init__(self, config: Config) -> None:
        self.p = config.dropout
