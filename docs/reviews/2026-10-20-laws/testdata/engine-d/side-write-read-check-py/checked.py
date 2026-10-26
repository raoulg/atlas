"""Write a file and read it back only to check it arrived."""

from pathlib import Path

OUT = Path("data/out.txt")


def run() -> None:
    OUT.write_text("hello")
    if OUT.read_text() != "hello":
        raise RuntimeError("lost")
