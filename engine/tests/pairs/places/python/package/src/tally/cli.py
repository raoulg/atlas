import argparse
from pathlib import Path


def count(source: Path) -> int:
    return len(source.read_text().splitlines())


def keep(total: int, out: Path) -> None:
    out.write_text(f"{total}\n")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, default=Path("data/lines.txt"))
    parser.add_argument("--out", type=Path, default=Path("out/count.txt"))
    args = parser.parse_args()
    keep(count(args.source), args.out)
