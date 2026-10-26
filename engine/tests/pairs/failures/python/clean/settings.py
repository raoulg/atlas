"""Settings read from one of several sources, chosen at run time. Every source fails as the base says a source fails,
with the program's own error, so the route that loads through the base hears of one error."""

from abc import ABC, abstractmethod


class SourceError(Exception):
    """A source that cannot give its settings."""


class Source(ABC):
    @abstractmethod
    def load(self) -> dict[str, str]:
        """The settings, or a SourceError."""


class FileSource(Source):
    def load(self) -> dict[str, str]:
        raise SourceError("no settings file")


class EnvSource(Source):
    def load(self) -> dict[str, str]:
        raise SourceError("no settings variable")


def settings(source: Source) -> dict[str, str]:
    return source.load()


def run() -> None:
    print(settings(FileSource()))
