"""The mean of some readings, scaled and shifted. The settings are built as objects, and the steps are functions beside
them: the caller hands each step the object, or takes the setting out of it, along with the readings."""

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


class Shift:
    def __init__(self, by: float) -> None:
        self.by = by


def scale_by(readings: Readings, by: float) -> Readings:
    return Readings([v * by for v in readings.values])


def shift_with(readings: Readings, shift: Shift) -> Readings:
    return Readings([v + shift.by for v in readings.values])


def mean(readings: Readings) -> Mean:
    return Mean(sum(readings.values) / len(readings.values))


def run() -> None:
    scale = Scale(2.0)
    shift = Shift(1.0)
    readings = Readings([1.0, 2.0, 3.0])
    scaled = scale_by(readings, scale.by)
    shifted = shift_with(scaled, shift)
    print(mean(shifted).value)
