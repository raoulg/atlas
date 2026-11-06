from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Report:
    title: str
    rows: list[str]


@dataclass(frozen=True)
class Receipt:
    path: Path
