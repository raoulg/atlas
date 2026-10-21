from pathlib import Path

from news.sources import Feed
from news.store import Store


def headlines(url: str, cache: Path) -> list[bytes]:
    feed = Feed(url, Store(cache))
    return feed.lines()
