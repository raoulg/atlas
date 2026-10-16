
"""Where finished reports are kept."""


class Sink:
    """Writes reports and gives nothing back."""

    def write(self, report: str, /) -> None:
        print(report)


class Store:
    """What goes in comes out."""

    def write(self, report: str, /) -> None: ...

    def read(self) -> str: ...
