"""Read one file of a directory and write another: two places, one in, one out."""

from pathlib import Path

DATA = Path("data")


def run() -> None:
    DATA.mkdir(exist_ok=True)
    text = (DATA / "in.txt").read_text()
    (DATA / "out.txt").write_text(text.upper())
