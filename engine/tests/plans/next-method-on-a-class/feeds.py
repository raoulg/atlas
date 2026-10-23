"""A feed is trimmed and logged, `Feed(Trimmed, Logged)`: on a feed, `Trimmed.read`'s `super().read()` runs `Logged.read`,
which writes the log whenever it runs. The route reads back what the read logged and adds the news to it."""

from pathlib import Path

LOG = Path("data/log.txt")


class Source:
    def read(self) -> str:
        return Path("data/news.txt").read_text()


class Trimmed(Source):
    def read(self) -> str:
        return super().read().strip()


class Logged(Source):
    def read(self) -> str:
        LOG.write_text("read\n")
        return super().read()


class Feed(Trimmed, Logged):
    pass


def run() -> None:
    feed = Feed()
    news = feed.read()
    LOG.write_text(LOG.read_text() + news)
