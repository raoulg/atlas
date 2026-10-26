"""List a pattern below a directory: the part of the directory the pattern holds."""

from pathlib import Path

ASSETS = Path("assets/regelingen")


def run() -> None:
    for m in ASSETS.glob("*/IO-*"):
        print(m.read_text())
