"""A harvest: the latest reading of each station, kept, and reports of what was kept."""

from dataclasses import dataclass
from pathlib import Path
from typing import Any

from harvest.client import WeatherClient
from harvest.export import exporter_for
from harvest.model import Fetched, Reading, convert
from harvest.store import ReadingStore

LATEST: dict[str, Reading] = {}


def remember(reading: Reading) -> None:
    LATEST[reading.station] = reading


@dataclass
class Harvester:
    """Fetches readings from the weather service, keeps them, and reports on them."""

    client: Any
    store: Any
    exporter: Any
    unit: str

    def collect(self, stations: list[str]) -> list[Fetched]:
        results = []
        for station in stations:
            result = self.client.latest(station)
            if isinstance(result, Reading):
                convert(result, self.unit)
                self.store.save(result)
            results.append(result)
        return results

    def report(self, station: str, directory: Path) -> Path:
        return self.exporter.export(self.store.history(station), directory, station)

    def run(self, kind: str, **kw: Any) -> Any:
        return getattr(self, kind)(**kw)


def build_harvester(config: dict) -> Harvester:
    return Harvester(
        client=WeatherClient(config.get("timeout", 10.0)),
        store=ReadingStore(config["database"]),
        exporter=exporter_for(config["format"]),
        unit=config["unit"],
    )
