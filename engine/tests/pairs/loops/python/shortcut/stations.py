"""Each station of a network, reported: the stations are the module's own list, not the network's."""

from dataclasses import dataclass


@dataclass
class Station:
    name: str


STATIONS: list[Station] = [Station("De Bilt"), Station("Eelde")]


def report(station: Station) -> None:
    print(station.name)


def report_all() -> None:
    for station in STATIONS:
        report(station)


def run() -> None:
    report_all()
