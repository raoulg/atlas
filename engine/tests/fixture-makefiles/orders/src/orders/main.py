from pathlib import Path


def main() -> None:
    Path("out/orders.txt").write_text("orders")
