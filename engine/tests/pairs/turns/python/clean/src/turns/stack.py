"""A stack of hidden layers built from its config: each turn of the widths builds a Linear, the activation the config
names and a Dropout, written where the turn runs; the activation is an entry of a table, looked up by the config."""

from dataclasses import dataclass
from itertools import pairwise
from types import SimpleNamespace

from turns.layers import GELU, Calibrate, Config, Dropout, Layer, Linear, Readout, ReLU, Tanh

ACTIVATIONS = {"gelu": GELU, "relu": ReLU, "tanh": Tanh}


@dataclass
class Stack:
    layers: list[Layer | SimpleNamespace]


def build(config: Config) -> Stack:
    return Stack(
        [
            layer
            for width_in, width_out in pairwise(config.widths)
            for layer in (
                Linear(width_in, width_out),
                ACTIVATIONS[config.activation](),
                Dropout(config.dropout),
                SimpleNamespace(scale=config.dropout),
            )
        ]
    )


def head(config: Config) -> Stack:
    layers: list[Layer] = []
    for layer in (Readout(config), Calibrate(config)):
        layers.append(layer)
    return Stack(layers)


def by_table(config: Config) -> Layer:
    return ACTIVATIONS[config.activation]()


def by_cases(config: Config) -> Layer:
    match config.activation:
        case "gelu":
            return GELU()
        case "relu":
            return ReLU()
        case "tanh":
            return Tanh()
    raise ValueError(config.activation)


def square(config: Config) -> Stack:
    return Stack([Linear(width, width) for width in config.widths])
