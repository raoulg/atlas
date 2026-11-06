from pathlib import Path

from notekit import save


def main() -> None:
    bills = Path("data/bills.csv").read_text()
    save("out/bills.txt", bills)
