"""Fill a cache where it is missing, then read it only to check it is not empty."""

from pathlib import Path

CACHE = Path("cache/data.txt")


def run() -> None:
    if not CACHE.exists():
        CACHE.write_text("fresh")
    if CACHE.read_text() == "":
        raise RuntimeError("empty cache")
