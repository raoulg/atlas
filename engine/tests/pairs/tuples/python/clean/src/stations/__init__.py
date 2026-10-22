"""The mean of each station's readings. A station's mean is a record with a name of its own, and the
station and its store come back together, where there is one."""

from dataclasses import dataclass

__all__ = ["Mean", "Station", "Store", "first", "means"]


@dataclass(frozen=True)
class Station:
    """A weather station."""

    id: str
    readings: tuple[float, ...]


@dataclass(frozen=True)
class Mean:
    """The mean of a station's readings."""

    station: Station
    mean: float


@dataclass(frozen=True)
class Store:
    """Where the stations are kept."""

    stations: tuple[Station, ...]


def means(stations: list[Station]) -> list[Mean]:
    """The mean of each station's readings."""
    return [Mean(station, sum(station.readings) / len(station.readings)) for station in stations]


def first(store: Store) -> tuple[Station, Store] | None:
    """The first station of a store, with the store itself, where it keeps one."""
    return (store.stations[0], store) if store.stations else None
