from shop.orders.lines import price


def total(n: int) -> int:
    return price(n) + 1


def tax(n: int) -> int:
    return n // 5
