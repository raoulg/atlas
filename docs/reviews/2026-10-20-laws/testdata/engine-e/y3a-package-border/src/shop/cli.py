from shop.pricing import priced
from shop.store import read_orders, save_orders


def main() -> None:
    save_orders(priced(read_orders()))
