"""Settings read from one of several sources, chosen at run time. Each source fails its own way, with whatever error
came to hand, so the route that loads through the base can fail in as many ways as there are sources."""

from abc import ABC, abstractmethod


class Source(ABC):
    @abstractmethod
    def load(self) -> dict[str, str]:
        """The settings."""


class FileSource(Source):
    def load(self) -> dict[str, str]:
        raise ValueError("no settings file")


class EnvSource(Source):
    def load(self) -> dict[str, str]:
        raise RuntimeError("no settings variable")


def settings(source: Source) -> dict[str, str]:
    return source.load()


def run() -> None:
    print(settings(FileSource()))
