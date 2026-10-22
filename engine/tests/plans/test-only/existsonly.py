"""Only test whether a flag file is there: no content is taken."""

from pathlib import Path

FLAG = Path("data/flag")


def run() -> None:
    if FLAG.exists():
        print("flag set")
