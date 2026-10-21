from dataclasses import dataclass
from pathlib import Path


@dataclass
class Frame:
    values: list[float]


def load(path: Path) -> Frame:
    return Frame([float(x) for x in path.read_text().split()])


def fetch(path: Path) -> Frame:
    return Frame([float(x) for x in path.read_text().split(",")])


def smooth(frame: Frame) -> Frame:
    return Frame(frame.values[1:])


def plot(frame: Frame) -> None:
    Path("out/plot.txt").write_text(str(frame.values))


def main() -> None:
    raw = fetch(Path("data/in.txt"))
    frame = smooth(raw)
    plot(frame)
