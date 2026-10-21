"""The dishes the shop offers, cooked to order."""


def dish(n: int) -> int:
    from kitchen.cook import boil

    return boil(n)
