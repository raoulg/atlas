"""The mean of some readings, tidied and scaled. Each step refuses a call that hands it nothing: a check written once,
`guarded`, and put around a function where the function is defined, by its name or by a call that gives it."""

from collections.abc import Callable
from dataclasses import dataclass
from typing import ParamSpec, TypeVar

P = ParamSpec("P")
R = TypeVar("R")


class Refused(Exception):
    """A call that hands a step nothing to work on."""


@dataclass
class Readings:
    values: list[float]


@dataclass
class Mean:
    value: float


def guarded(fn: Callable[P, R]) -> Callable[P, R]:
    def inner(*args: P.args, **kwargs: P.kwargs) -> R:
        if not args:
            raise Refused(fn.__name__)
        return fn(*args, **kwargs)

    return inner


def strict(least: int) -> Callable[[Callable[P, R]], Callable[P, R]]:
    def wrap(fn: Callable[P, R]) -> Callable[P, R]:
        def inner(*args: P.args, **kwargs: P.kwargs) -> R:
            if len(args) < least:
                raise Refused(fn.__name__)
            return fn(*args, **kwargs)

        return inner

    return wrap


class Scale:
    def __init__(self, by: float) -> None:
        self.by = by

    @guarded
    def apply(self, readings: Readings) -> Readings:
        return Readings([v * self.by for v in readings.values])


@guarded
def tidy(readings: Readings) -> Readings:
    return Readings([v for v in readings.values if v >= 0])


@strict(least=1)
def mean(readings: Readings) -> Mean:
    return Mean(sum(readings.values) / len(readings.values))


def run() -> None:
    scale = Scale(2.0)
    readings = Readings([1.0, -2.0, 3.0])
    print(mean(scale.apply(tidy(readings))).value)
