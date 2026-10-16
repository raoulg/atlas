
"""Where finished reports are kept."""

from archive import prophecy


class Sink:
    """Writes reports and gives nothing back."""

    def write(self, report: str, /) -> None:
        print(report)


@prophecy(supersedes="archive.sink.Sink")
class Store:
    """What goes in comes out."""

    def write(self, report: str, /) -> None: ...

    def read(self) -> str: ...
