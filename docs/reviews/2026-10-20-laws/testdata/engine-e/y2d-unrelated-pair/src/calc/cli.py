from dataclasses import dataclass
from pathlib import Path


@dataclass
class Frame:
    values: list[float]


def load(path: Path) -> Frame:
    return Frame([float(x) for x in path.read_text().split()])


def label(name: str) -> str:
    return name.upper() + "!"


def main() -> None:
    frame = load(Path("data/in.txt"))
    print(label("raw"))
