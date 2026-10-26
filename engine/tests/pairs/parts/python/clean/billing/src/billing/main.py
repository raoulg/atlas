from pathlib import Path


def bill(text: str) -> str:
    return text.upper()


def main() -> None:
    Path("out/invoice.txt").write_text(bill(Path("data/ledger.csv").read_text()))
