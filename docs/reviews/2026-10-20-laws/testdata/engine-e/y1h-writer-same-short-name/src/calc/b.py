from pathlib import Path


def save() -> None:
    Path("out/x.txt").write_text("b")
