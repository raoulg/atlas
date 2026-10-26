"""Stage a payload in a folder of its own, then read a card from wherever the environment says. The payload's metadata
goes to the folder the caller makes, the card comes from a file no code names: two JSON files, neither the other, so the
card is read from the world, not read back."""

import json
import os
from pathlib import Path
from tempfile import TemporaryDirectory
from typing import TextIO


def stage(dest: Path) -> None:
    with open(dest / "meta.json", "w") as f:
        json.dump({"split": "train"}, f)


def card(f: TextIO) -> dict[str, str]:
    return json.load(f)


def publish() -> dict[str, str]:
    with TemporaryDirectory() as tmp:
        stage(Path(tmp))
    with open(os.environ["CARD"]) as f:
        return card(f)


def run() -> None:
    print(publish())
