from dataclasses import dataclass
from pathlib import Path


@dataclass
class Meta:
    name: str


@dataclass
class Frame:
    values: list[float]
    count: int = 0


def load(path: Path) -> Frame:
    return Frame([float(x) for x in path.read_text().split()])


def total(frame: Frame) -> float:
    return sum(frame.values)


def describe(meta: Meta) -> str:
    return meta.name


def main() -> None:
    frame = load(Path("data/in.txt"))
    print(total(frame))
    print(describe(Meta("raw")))
