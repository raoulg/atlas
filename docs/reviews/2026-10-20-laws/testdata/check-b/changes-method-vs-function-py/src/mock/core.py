from dataclasses import dataclass, field


@dataclass
class Bag:
    items: list[str] = field(default_factory=list)


def fn_clear(bag: Bag) -> int:
    bag.items.clear()
    return 1


def fn_assign(bag: Bag) -> int:
    bag.items = []
    return 1

class Keeper:
    def m_clear(self, bag: Bag) -> int:
        bag.items.clear()
        return 1

    def m_assign(self, bag: Bag) -> int:
        bag.items = []
        return 1


def main() -> int:
    k = Keeper()
    return fn_clear(Bag()) + fn_assign(Bag()) + k.m_clear(Bag()) + k.m_assign(Bag())
