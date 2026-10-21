from dataclasses import dataclass
from pathlib import Path


@dataclass
class Frame:
    values: list[float]


def load(path: Path) -> Frame:
    return Frame([float(x) for x in path.read_text().split()])


def alpha(frame: Frame) -> Frame:
    return frame


def beta(frame: Frame) -> Frame:
    return frame


def main() -> None:
    frame = load(Path("data/in.txt"))
    alpha(frame)
    beta(frame)
