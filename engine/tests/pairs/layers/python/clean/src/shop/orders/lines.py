from shop.orders.tax import tax


def price(n: int) -> int:
    return n + tax(n)
