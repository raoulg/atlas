"""Only check a config file is not empty; nothing is written."""

from pathlib import Path

CFG = Path("data/config.txt")


def run() -> None:
    if CFG.read_text() == "":
        raise RuntimeError("empty config")
    print("ok")
