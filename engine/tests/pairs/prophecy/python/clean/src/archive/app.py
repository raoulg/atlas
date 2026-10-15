from archive import prophecy

from archive.sink import Sink, Store


def run(report: str) -> None:
    Sink().write(report)


@prophecy(supersedes="archive.app.run")
def run_stored(report: str) -> None:
    Store().write(report)
