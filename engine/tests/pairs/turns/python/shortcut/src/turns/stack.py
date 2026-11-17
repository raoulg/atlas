"""A stack of hidden layers built from its config: the layers of a turn and the activation are found by their names in
the module of layers, at run time; the tables an activation is looked up in are changed by the module, looked up by a
constant, bound again where they are read, or hold one entry."""

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


REGISTRY = {"gelu": layers.GELU, "relu": layers.ReLU}
DEFAULT = "gelu"
FIXED = {"gelu": layers.GELU, "relu": layers.ReLU}
ONE = {"gelu": layers.GELU}


def register(name: str, cls: type[Layer]) -> None:
    REGISTRY[name] = cls


register("tanh", layers.Tanh)


def registered(config: Config) -> Layer:
    return REGISTRY[config.activation]()


def by_default() -> Layer:
    return FIXED[DEFAULT]()


def each_table(tables: list[dict[str, type[Layer]]], config: Config) -> list[Layer]:
    made: list[Layer] = []
    for FIXED in tables:
        made.append(FIXED[config.activation]())
    return made


def only(config: Config) -> Layer:
    return ONE[config.activation]()
