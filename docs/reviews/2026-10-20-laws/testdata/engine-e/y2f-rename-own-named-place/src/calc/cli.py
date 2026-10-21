from pathlib import Path


def draw(values: list[float], out: Path) -> None:
    out.write_text(str(values))


def main() -> None:
    print(sum([1.0, 2.0]))
