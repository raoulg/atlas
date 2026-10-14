"""What a harvest is about: a station's reading, or why there is none."""

from dataclasses import dataclass, replace
from datetime import date, datetime
from enum import Enum


class Unit(Enum):
    """The unit a temperature is reported in; the weather service reports Celsius."""

    CELSIUS = "celsius"
    FAHRENHEIT = "fahrenheit"

    @property
    def symbol(self) -> str:
        match self:
            case Unit.CELSIUS:
                return "°C"
            case Unit.FAHRENHEIT:
                return "°F"

    def from_celsius(self, degrees: float) -> float:
        match self:
            case Unit.CELSIUS:
                return degrees
            case Unit.FAHRENHEIT:
                return degrees * 9 / 5 + 32


@dataclass(frozen=True)
class Reading:
    station: str
    taken_at: datetime
    temperature: float
    humidity: float
    unit: Unit = Unit.CELSIUS


@dataclass(frozen=True)
class Outage:
    """A station the service gave no reading for, and what it said instead."""

    station: str
    reason: str


@dataclass(frozen=True)
class Day:
    """A station on a date: what a summary is of."""

    station: str
    date: date


@dataclass(frozen=True)
class Summary:
    """The lowest, highest and mean temperature a station had on a date."""

    station: str
    date: date
    low: float
    high: float
    mean: float


@dataclass(frozen=True)
class Harvest:
    """What one harvest kept, and the stations it could not reach."""

    readings: list[Reading]
    outages: list[Outage]


def in_unit(reading: Reading, unit: Unit) -> Reading:
    """A reading the service gave in Celsius, with its temperature in `unit`."""
    return replace(reading, temperature=unit.from_celsius(reading.temperature), unit=unit)


def day_summary(day: Day, readings: list[Reading]) -> Summary:
    """The summary of the readings taken on the day."""
    temperatures = [r.temperature for r in readings if r.taken_at.date() == day.date]
    return Summary(day.station, day.date, min(temperatures), max(temperatures), sum(temperatures) / len(temperatures))
