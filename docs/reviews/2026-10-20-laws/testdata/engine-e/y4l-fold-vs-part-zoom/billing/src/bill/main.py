from pathlib import Path


def main() -> None:
    Path("out/a/8.txt").write_text(Path("data/ledger.csv").read_text())
