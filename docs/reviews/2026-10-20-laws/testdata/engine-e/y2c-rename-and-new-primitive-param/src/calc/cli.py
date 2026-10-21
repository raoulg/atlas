from dataclasses import dataclass
from pathlib import Path


@dataclass
class Frame:
    values: list[float]


def load(path: Path) -> Frame:
    return Frame([float(x) for x in path.read_text().split()])


def draw(frame: Frame, out: Path, width: int) -> None:
    out.write_text(str(frame.values)[:width])


def main() -> None:
    frame = load(Path("data/in.txt"))
    draw(frame, Path("out/a.txt"), 80)
