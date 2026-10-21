"""Each station of a network, reported: the stations come from the network the route is handed."""

from dataclasses import dataclass
from pathlib import Path


@dataclass
class Station:
    name: str


@dataclass
class Network:
    stations: list[Station]


def load(path: Path) -> Network:
    return Network([Station(line) for line in path.read_text().splitlines()])


def report(station: Station) -> None:
    print(station.name)


def report_all(network: Network) -> None:
    for station in network.stations:
        report(station)


def run() -> None:
    report_all(load(Path("stations.txt")))
