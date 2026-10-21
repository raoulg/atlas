"""Only make the build directory: it gets no content."""

from pathlib import Path

BUILD = Path("build")


def run() -> None:
    BUILD.mkdir(parents=True, exist_ok=True)
