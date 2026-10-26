from pathlib import Path

from news.sources import Cached, Feed, Trimmed
from news.store import Store


def headlines(url: str, cache: Path) -> list[bytes]:
    feed = Feed(url, Store(cache))
    return feed.read().splitlines()


def lines(source: Trimmed) -> list[bytes]:
    return source.read().splitlines()


def cached_lines(source: Cached) -> list[bytes]:
    return source.read().splitlines()
