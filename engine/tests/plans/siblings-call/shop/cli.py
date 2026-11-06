"""The command: bill a cart. Its functions stand in one module, side by side."""

from dataclasses import dataclass


@dataclass
class Cart:
    items: list[float]


def main() -> None:
    cart = Cart([1.0, 2.5])
    total(cart)
    report(cart)
    stamp(cart)
    print(discount(cart))


def total(cart: Cart) -> None:
    print(price(cart) + tax(cart))


def report(cart: Cart) -> None:
    print(price(cart), tax(cart))


def stamp(cart: Cart) -> None:
    print(len(cart.items))


def price(cart: Cart) -> float:
    return sum(cart.items)


def tax(cart: Cart) -> float:
    return sum(cart.items) * 0.21


def discount(cart: Cart) -> float:
    return sum(cart.items) * 0.1
