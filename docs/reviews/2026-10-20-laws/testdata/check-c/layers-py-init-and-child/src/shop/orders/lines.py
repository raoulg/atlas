def price(n: int) -> int:
    from shop.orders import tax

    return n + tax(n)
