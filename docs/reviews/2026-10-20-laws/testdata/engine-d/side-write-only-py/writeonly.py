"""Write a file: the file is where the route leaves its result."""

from pathlib import Path

OUT = Path("data/out.txt")


def run() -> None:
    OUT.write_text("hello")
