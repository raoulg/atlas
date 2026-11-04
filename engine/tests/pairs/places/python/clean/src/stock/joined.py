"""Counts kept by kind under the folder whoever calls names, each path joined one way Python joins one."""

import json
import os
from pathlib import Path


def joined(data: str, kind: str) -> dict[str, int]:
    with open(os.path.join(data, kind, "counts.json")) as f:
        return json.load(f)


def added(data: str, kind: str) -> dict[str, int]:
    with open(data + "/" + kind + "/counts.json") as f:
        return json.load(f)


def divided(data: Path, kind: str) -> dict[str, int]:
    with open(data / kind / "counts.json") as f:
        return json.load(f)


def dumped(data: str, kind: str, counts: dict[str, int]) -> None:
    with open(os.path.join(data, kind, "counts.json"), "w") as f:
        json.dump(counts, f)
