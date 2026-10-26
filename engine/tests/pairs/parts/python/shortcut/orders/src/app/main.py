from pathlib import Path


def take_orders() -> str:
    return Path("data/orders.csv").read_text()


def main() -> None:
    Path("out/orders.txt").write_text(take_orders())
