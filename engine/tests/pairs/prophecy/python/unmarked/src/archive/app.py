from archive.sink import Sink, Store


def run(report: str) -> None:
    Sink().write(report)
