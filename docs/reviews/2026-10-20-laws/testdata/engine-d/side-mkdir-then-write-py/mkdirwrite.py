"""Make the build directory, then write a report into it: the directory gets no content."""

from pathlib import Path

BUILD = Path("build")


def run() -> None:
    BUILD.mkdir(parents=True, exist_ok=True)
    (BUILD / "report.txt").write_text("ok")
