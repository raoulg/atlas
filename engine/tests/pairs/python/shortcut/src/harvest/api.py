"""The HTTP API: a station's kept readings, the latest of this process, and tasks by name."""

import os
from typing import Any

from fastapi import FastAPI

from harvest.harvester import LATEST, build_harvester
from harvest.model import Reading
from harvest.settings import load_settings
from harvest.store import ReadingStore

app = FastAPI()


def get_store():
    return ReadingStore(os.environ.get("HARVEST_DB", "data/harvest.db"))


@app.get("/stations/{station}/readings")
def readings(station: str) -> list[dict]:
    return get_store().history(station)


@app.get("/stations/{station}/latest")
def latest(station: str) -> Reading | None:
    return LATEST.get(station)


@app.post("/tasks/{kind}")
def run_task(kind: str, params: dict[str, Any]) -> Any:
    return build_harvester(load_settings()).run(kind, **params)
