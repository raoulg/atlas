"""What the program makes is kept in a folder of the project's own, the workbench: a class that
holds the folder, whose method names each file in it."""

from pathlib import Path


class Workbench:
    def __init__(self, root: Path) -> None:
        self.root = root

    @classmethod
    def of(cls, project: Path) -> "Workbench":
        return cls(project / ".made")

    def file(self, name: str) -> Path:
        return self.root / name


def save_report(project: Path, text: str) -> None:
    Workbench.of(project).file("report.txt").write_text(text)


def save_log(project: Path, text: str) -> None:
    bench = Workbench.of(project)
    bench.file("log.txt").write_text(text)
