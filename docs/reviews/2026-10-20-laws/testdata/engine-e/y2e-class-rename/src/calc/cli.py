from pathlib import Path


class Series:
    def __init__(self, values: list[float]) -> None:
        self.values = values

    def mean(self) -> float:
        return sum(self.values) / len(self.values)

    def trimmed(self) -> "Series":
        return Series(self.values[1:])


def load(path: Path) -> Series:
    return Series([float(x) for x in path.read_text().split()])


def smooth(frame: Series) -> Series:
    return frame.trimmed()


def main() -> None:
    frame = smooth(load(Path("data/in.txt")))
    print(frame.mean())
