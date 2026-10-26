from pathlib import Path


class Frame:
    def __init__(self, values: list[float]) -> None:
        self.values = values


def load(path: Path) -> Frame:
    return Frame([float(x) for x in path.read_text().split()])


def smooth(frame: Frame) -> Frame:
    return Frame(frame.values[1:])



class Trend:
    def __init__(self, frame: Frame) -> None:
        self.frame = frame

    def save(self, out: Path) -> None:
        out.write_text(" ".join(str(v) for v in self.frame.values))


def main() -> None:
    frame = smooth(load(Path("data/raw.csv")))
    Trend(frame).save(Path("figures/spots.txt"))
