"""The HTTP API: a station's kept readings, and a harvest on request."""

from fastapi import FastAPI

from harvest.harvester import build_harvester
from harvest.locations import API_URL, CONFIG_FILE, DATABASE, STATIONS_FILE
from harvest.model import Harvest, Reading
from harvest.settings import load_settings
from harvest.stations import read_stations
from harvest.store import ReadingStore

app = FastAPI()


def get_store() -> ReadingStore:
    return ReadingStore(DATABASE)


@app.get("/stations/{station}/readings")
def readings(station: str) -> list[Reading]:
    return get_store().history(station)


@app.post("/collect")
def collect() -> Harvest:
    harvester = build_harvester(load_settings(CONFIG_FILE), DATABASE, API_URL)
    return harvester.collect(read_stations(STATIONS_FILE))
