from pathlib import Path


def load(path: Path) -> list[float]:
    return [float(x) for x in path.read_text().split()]


def main() -> None:
    xs = [1.0, 2.0]
    print(sum(xs))
