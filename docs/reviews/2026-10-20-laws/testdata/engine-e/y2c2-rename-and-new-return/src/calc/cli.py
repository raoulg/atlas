from dataclasses import dataclass
from pathlib import Path


@dataclass
class Frame:
    values: list[float]


def load(path: Path) -> Frame:
    return Frame([float(x) for x in path.read_text().split()])


def draw(frame: Frame, out: Path) -> str:
    out.write_text(str(frame.values))
    return str(out)


def main() -> None:
    frame = load(Path("data/in.txt"))
    print(draw(frame, Path("out/a.txt")))
