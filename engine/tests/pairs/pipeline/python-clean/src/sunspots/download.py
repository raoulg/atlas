"""A file from the web, kept as it came."""

from pathlib import Path

import requests


def download(url: str, to: Path) -> Path:
    """Fetch `url` and write its body to `to`; returns `to`."""
    response = requests.get(url, timeout=30)
    response.raise_for_status()
    to.parent.mkdir(parents=True, exist_ok=True)
    to.write_bytes(response.content)
    return to
