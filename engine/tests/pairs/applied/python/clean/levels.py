"""The mean of some readings, scaled and shifted. A step that needs a setting is built with it and then run: the
setting stays inside the step that holds it."""

from dataclasses import dataclass


@dataclass
class Readings:
    values: list[float]


@dataclass
class Mean:
    value: float


class Scale:
    def __init__(self, by: float) -> None:
        self.by = by

    def apply(self, readings: Readings) -> Readings:
        return Readings([v * self.by for v in readings.values])


class Shift:
    def __init__(self, by: float) -> None:
        self.by = by

    def apply(self, readings: Readings) -> Readings:
        return Readings([v + self.by for v in readings.values])


def mean(readings: Readings) -> Mean:
    return Mean(sum(readings.values) / len(readings.values))


def run() -> None:
    scale = Scale(2.0)
    shift = Shift(1.0)
    readings = Readings([1.0, 2.0, 3.0])
    scaled = scale.apply(readings)
    shifted = shift.apply(scaled)
    print(mean(shifted).value)
