"""The data directory comes from the environment: the code does not decide it, so it is a hole."""

import os
from pathlib import Path


def run() -> None:
    root = Path(os.environ["DATA_DIR"])
    print((root / "report.txt").read_text())
