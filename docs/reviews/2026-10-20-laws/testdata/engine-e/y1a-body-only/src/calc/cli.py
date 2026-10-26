from pathlib import Path


def load(path: Path) -> list[float]:
    return [float(x) for x in path.read_text().split()]


def total(xs: list[float]) -> float:
    return round(sorted(xs)[0] + sum(xs), 2)


def main() -> None:
    xs = load(Path("data/in.txt"))
    print(total(xs))
