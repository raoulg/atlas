"""A harvest: the latest reading of each station, kept, and reports of what was kept."""

from dataclasses import dataclass
from pathlib import Path

from harvest.client import StationUnavailable, WeatherClient
from harvest.export import Exporter, exporter_for
from harvest.model import Harvest, Outage, Reading, Unit, in_unit
from harvest.settings import Settings
from harvest.store import ReadingStore


@dataclass
class Harvester:
    """Fetches readings from the weather service, keeps them, and reports on them."""

    client: WeatherClient
    store: ReadingStore
    exporter: Exporter[Reading]
    unit: Unit

    def collect(self, stations: list[str]) -> Harvest:
        readings: list[Reading] = []
        outages: list[Outage] = []
        for station in stations:
            try:
                reading = in_unit(self.client.latest(station), self.unit)
            except StationUnavailable as error:
                outage = Outage(station, str(error))
                self.store.save_outage(outage)
                outages.append(outage)
                continue
            self.store.save(reading)
            readings.append(reading)
        return Harvest(readings, outages)

    def report(self, station: str, directory: Path) -> Path:
        return self.exporter.export(self.store.history(station), directory, station)


def build_harvester(settings: Settings, database: Path, api_url: str) -> Harvester:
    """A harvester for the service at `api_url` that keeps its readings in `database`."""
    return Harvester(
        client=WeatherClient(api_url, settings.timeout),
        store=ReadingStore(database),
        exporter=exporter_for(settings.report_format),
        unit=settings.unit,
    )
