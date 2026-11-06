"""The data: a batch, its labels and a table, each a value a library makes. The batch and the labels are torch's, the
table numpy's."""

import numpy
from jaxtyping import Float, Int
from torch import Tensor


def batches() -> Float[Tensor, "rows cols"]:
    raise NotImplementedError


def labels() -> Int[Tensor, "rows"]:
    raise NotImplementedError


def raw() -> Tensor:
    raise NotImplementedError


def table() -> numpy.ndarray:
    raise NotImplementedError
