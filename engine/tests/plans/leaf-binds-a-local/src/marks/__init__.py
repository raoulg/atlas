"""The loud samples of a signal, masked: `_loud` makes the mask from the samples itself, `_at_floor` compares them with the
module's floor."""

from torch import Tensor

__all__ = ["floor", "peaks"]

FLOOR = Tensor()


def _loud(samples: Tensor) -> Tensor:
    loud = samples.abs() > 0
    return loud & ~loud.all()


def _at_floor(samples: Tensor) -> Tensor:
    return samples == FLOOR


def peaks(samples: Tensor) -> None:
    mask = _loud(samples)
    print(samples * mask)


def floor(samples: Tensor) -> None:
    mask = _at_floor(samples)
    print(samples * mask)
