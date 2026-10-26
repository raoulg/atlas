"""List folders by a pattern and read in each: the listing and what is read in it are one folder."""

from pathlib import Path

ASSETS = Path("assets/regelingen")


def run() -> None:
    for io_dir in ASSETS.glob("*/IO-*"):
        print((io_dir / "Metadata.xml").read_text())
