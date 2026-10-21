"""Read four files of a directory and write three others: none is both read and written."""

from pathlib import Path

DATA = Path("data")


def run() -> None:
    a = (DATA / "a1.txt").read_text() + (DATA / "a2.txt").read_text()
    b = (DATA / "a3.txt").read_text() + (DATA / "a4.txt").read_text()
    (DATA / "b1.txt").write_text(a)
    (DATA / "b2.txt").write_text(b)
    (DATA / "b3.txt").write_text(a + b)
