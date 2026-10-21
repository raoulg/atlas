from dataclasses import dataclass
from pathlib import Path


@dataclass
class Frame:
    values: list[float]


def load(path: Path) -> Frame:
    return Frame([float(x) for x in path.read_text().split()])


def head(frame: Frame) -> Frame:
    return Frame(frame.values[:1])


def middle(frame: Frame) -> Frame:
    return Frame(frame.values[1:-1])


def tail(frame: Frame) -> Frame:
    return Frame(frame.values[-1:])


def report(frame: Frame) -> None:
    first = head(frame)
    mid = middle(frame)
    last = tail(frame)
    Path("out/report.txt").write_text(str(first.values + mid.values + last.values))


def main() -> None:
    frame = load(Path("data/in.txt"))
    report(frame)
