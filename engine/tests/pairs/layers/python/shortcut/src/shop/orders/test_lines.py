from kitchen.cook import boil
from shop.orders.lines import price


def boiled(n: int) -> int:
    return boil(n)


def test_a_line_is_priced_with_its_tax() -> None:
    assert price(5) == 6
    assert boiled(5) == 14
