from archive import prophecy

from archive.sink import Sink, Store


def run(report: str) -> None:
    Store().write(report)
