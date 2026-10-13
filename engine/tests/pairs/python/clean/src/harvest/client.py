"""The weather service's HTTP API."""

from datetime import datetime

import requests

from harvest.model import Reading


class StationUnavailable(Exception):
    """The service has no current reading for a station."""


class WeatherClient:
    """Asks the weather service at `base_url` for a station's latest observation."""

    def __init__(self, base_url: str, timeout: float) -> None:
        self.base_url = base_url
        self.timeout = timeout
        self.session = requests.Session()

    def latest(self, station: str) -> Reading:
        response = self.session.get(f"{self.base_url}/stations/{station}/observations/latest", timeout=self.timeout)
        if not response.ok:
            raise StationUnavailable(f"{station}: HTTP {response.status_code}")
        body = response.json()
        return Reading(
            station=station,
            taken_at=datetime.fromisoformat(body["timestamp"]),
            temperature=float(body["temperature"]),
            humidity=float(body["humidity"]),
        )
