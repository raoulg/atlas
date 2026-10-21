from pathlib import Path

from news.sources import Feed, Trimmed
from news.store import Store


def headlines(url: str, cache: Path) -> list[bytes]:
    feed = Feed(url, Store(cache))
    return feed.read().splitlines()


def lines(source: Trimmed) -> list[bytes]:
    return source.read().splitlines()
