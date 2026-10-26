"""A worker that closes its log when its work is done, and names it gives back trimmed."""

from pathlib import Path


class Closing:
    def __init__(self, log: Path) -> None:
        self.log = log

    def close(self) -> None:
        self.log.write_text("closed\n")


class Worker(Closing):
    def work(self, names: list[str]) -> int:
        kept = [trim(n) for n in names]
        self.close()
        return len(kept)


def trim(name: str) -> str:
    return name.strip()
