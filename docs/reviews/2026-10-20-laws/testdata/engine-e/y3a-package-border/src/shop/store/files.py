from pathlib import Path

from shop.model import Order


def line(order: Order) -> str:
    return f"{order.item},{order.amount}"


def read_orders() -> list[Order]:
    rows = Path("data/orders.csv").read_text().splitlines()
    return [Order(r.split(",")[0], float(r.split(",")[1])) for r in rows]


def save_orders(orders: list[Order]) -> None:
    Path("out/orders.csv").write_text("\n".join(line(o) for o in orders))
