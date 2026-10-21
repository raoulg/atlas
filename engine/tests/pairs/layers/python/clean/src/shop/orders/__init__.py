"""What an order of n items costs."""

from shop.orders.lines import price


def total(n: int) -> int:
    return price(n) + 1
