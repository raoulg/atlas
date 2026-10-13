"""Writes a report where the command line says, by default under `out/`."""

import argparse
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("-o", "--out-file", default="out/report.txt")
    parser.add_argument("title")
    args = parser.parse_args()
    Path(args.out_file).write_text(args.title)


if __name__ == "__main__":
    main()
