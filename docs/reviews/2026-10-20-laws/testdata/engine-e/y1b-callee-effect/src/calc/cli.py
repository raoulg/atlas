from pathlib import Path


def load(path: Path) -> list[float]:
    return [float(x) for x in path.read_text().split()]


def save(xs: list[float]) -> None:
    Path("out/b.txt").write_text(" ".join(str(x) for x in xs))


def main() -> None:
    xs = load(Path("data/in.txt"))
    save(xs)
