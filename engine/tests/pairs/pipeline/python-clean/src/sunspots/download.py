"""A file from the web, kept as it came."""

from pathlib import Path

import requests

from sunspots.series import RAW


def download(url: str, to: Path = RAW) -> Path:
    """Fetch `url` and write its body to `to`, where `load` reads by default; returns `to`."""
    response = requests.get(url, timeout=30)
    response.raise_for_status()
    to.parent.mkdir(parents=True, exist_ok=True)
    to.write_bytes(response.content)
    return to
