"""The shelf on disk: one title on each line."""

from pathlib import Path


def load(path: Path) -> list[str]:
    return path.read_text().splitlines()
