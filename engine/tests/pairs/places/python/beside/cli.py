import argparse
from pathlib import Path

from tally import count, keep


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, default=Path("data/lines.txt"))
    parser.add_argument("--out", type=Path, default=Path("out/count.txt"))
    args = parser.parse_args()
    keep(count(args.source), args.out)
