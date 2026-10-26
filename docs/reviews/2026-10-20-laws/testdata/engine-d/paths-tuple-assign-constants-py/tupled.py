"""Three module constants assigned in one line: each is a file the code names."""

from pathlib import Path

SRC, OUT = Path("data/in.txt"), Path("data/out.txt")


def run() -> None:
    OUT.write_text(SRC.read_text().upper())
