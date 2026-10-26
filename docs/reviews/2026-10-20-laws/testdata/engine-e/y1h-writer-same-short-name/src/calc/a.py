from pathlib import Path


def save() -> None:
    Path("out/y.txt").write_text("a")
