from dataclasses import dataclass
from pathlib import Path


@dataclass
class Meta:
    name: str


@dataclass
class Frame:
    values: list[float]
    count: int
    meta: Meta


def load(path: Path) -> Frame:
    xs = [float(x) for x in path.read_text().split()]
    return Frame(xs, len(xs), Meta("raw"))


def total(frame: Frame, scale: int) -> float:
    return sum(frame.values) * scale


def main() -> None:
    frame = load(Path("data/in.txt"))
    print(total(frame, 2))
