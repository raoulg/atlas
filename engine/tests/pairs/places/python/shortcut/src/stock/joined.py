"""Counts kept by kind in a file the module names, its path joined each way Python joins one."""

import json
import os
from pathlib import Path


def joined(kind: str) -> dict[str, int]:
    with open(os.path.join("data", kind, "counts.json")) as f:
        return json.load(f)


def added(kind: str) -> dict[str, int]:
    with open("data/" + kind + "/counts.json") as f:
        return json.load(f)


def divided(kind: str) -> dict[str, int]:
    with open(Path("data") / kind / "counts.json") as f:
        return json.load(f)


def dumped(kind: str, counts: dict[str, int]) -> None:
    with open(os.path.join("data", kind, "counts.json"), "w") as f:
        json.dump(counts, f)
