"""Fill a cache on first use, then read it: the cache is made sure of, not changed."""

from pathlib import Path

CACHE = Path("cache/data.txt")


def run() -> None:
    if not CACHE.exists():
        CACHE.write_text("fresh")
    data = CACHE.read_text()
    print(data)
