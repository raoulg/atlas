from pathlib import Path

from notekit import save


def main() -> None:
    orders = Path("data/orders.csv").read_text()
    save("out/orders.txt", orders)
