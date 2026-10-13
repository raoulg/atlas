"""The weather service's HTTP API."""

import os

import requests

from harvest.model import Fetched, Outage, Reading

API_URL = os.environ.get("WEATHER_API_URL", "https://api.weather.example/v1")


class WeatherClient:
    """Asks the weather service for a station's latest observation."""

    def __init__(self, timeout: float = 10.0) -> None:
        self.timeout = timeout
        self.session = requests.Session()

    def latest(self, station: str) -> Fetched:
        response = self.session.get(f"{API_URL}/stations/{station}/observations/latest", timeout=self.timeout)
        if not response.ok:
            return Outage(station, f"HTTP {response.status_code}")
        body = response.json()
        return Reading(station, body["timestamp"], body["temperature"], body["humidity"])
