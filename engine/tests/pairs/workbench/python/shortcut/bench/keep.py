"""What the program makes is kept in a folder of the project's own, handed around as a bare path."""
import shutil
from pathlib import Path


def made_dir(project: Path) -> Path:
    return project / ".made"


def save_report(project: Path, text: str) -> None:
    (made_dir(project) / "report.txt").write_text(text)


def save_log(project: Path, text: str) -> None:
    folder = made_dir(project)
    (folder / "log.txt").write_text(text)


def clear(project: Path) -> None:
    shutil.rmtree((made_dir(project) / "report.txt").parent)
