"""Directories the environment names: the code does not decide them."""

import os
from pathlib import Path


def run() -> None:
    print((Path(os.environ["DATA_DIR"]) / "sub" / "report.txt").read_text())
    print((Path(os.environ.get("CACHE_DIR", "cache")) / "index.json").read_text())
    print((Path(os.getenv("OUT_DIR")) / "out.csv").read_text())
