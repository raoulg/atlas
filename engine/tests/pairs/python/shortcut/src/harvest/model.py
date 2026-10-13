"""What a harvest is about: a station's reading, or why there is none."""

from dataclasses import dataclass


@dataclass
class Reading:
    station: str
    taken_at: str
    temperature: float
    humidity: float
    unit: str = "°C"


@dataclass
class Outage:
    """A station the service gave no reading for, and what it said instead."""

    station: str
    reason: str


Fetched = Reading | Outage


def convert(reading: Reading, unit: str) -> None:
    """Converts a reading the service gave in Celsius to `unit`, in place."""
    if unit == "fahrenheit":
        reading.temperature = reading.temperature * 9 / 5 + 32
    reading.unit = "°F" if unit == "fahrenheit" else "°C"
