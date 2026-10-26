"""A worker that closes when its work is done, and trims the names it is handed."""

from pathlib import Path


class Closing:
    def __init__(self, log: Path) -> None:
        self.log = log

    def close(self) -> None:
        pass


class Worker(Closing):
    def work(self, names: list[str]) -> int:
        for n in names:
            trim(n)
        self.close()
        return len(names)


def trim(name: str) -> None:
    name.strip()
