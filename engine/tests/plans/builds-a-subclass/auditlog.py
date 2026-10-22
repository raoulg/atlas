"""Build a subclass that has no initializer of its own: building it runs its base's, and none of its methods."""

from pathlib import Path


def stamp(path: Path) -> None:
    path.write_text("opened\n")


class Log:
    def __init__(self, path: Path) -> None:
        self.path = path
        stamp(path)

    def add(self, line: str) -> None:
        with self.path.open("a") as f:
            f.write(line)


class Audit(Log):
    def last(self) -> str:
        return self.path.read_text().splitlines()[-1]


def run(path: Path) -> None:
    audit = Audit(path)
    print(audit.path)
