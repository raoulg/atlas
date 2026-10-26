from shop.model import Order


def priced(orders: list[Order]) -> list[Order]:
    return [Order(o.item, o.amount * 1.21) for o in orders]
