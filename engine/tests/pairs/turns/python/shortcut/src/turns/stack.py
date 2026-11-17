"""A stack of hidden layers built from its config: the layers of a turn and the activation are found by their names in
the module of layers, at run time."""

from dataclasses import dataclass
from itertools import pairwise

from turns import layers
from turns.layers import Config, Dropout, Layer, Linear


@dataclass
class Stack:
    layers: list[Layer]


def build(config: Config) -> Stack:
    made: list[Layer] = []
    for width_in, width_out in pairwise(config.widths):
        made.append(Linear(width_in, width_out))
        made.append(getattr(layers, config.activation)())
        made.append(Dropout(config.dropout))
    return Stack(made)


def head(config: Config) -> Stack:
    made: list[Layer] = []
    for name in ("Readout", "Calibrate"):
        made.append(getattr(layers, name)(config))
    return Stack(made)
