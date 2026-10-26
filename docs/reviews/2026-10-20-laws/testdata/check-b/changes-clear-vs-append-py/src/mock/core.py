from dataclasses import dataclass, field


@dataclass
class Bag:
    items: list[str] = field(default_factory=list)

    def empty(self) -> int:
        self.items.clear()
        return 1


def by_clear(bag: Bag) -> int:
    bag.items.clear()
    return 1


def by_append(bag: Bag) -> int:
    bag.items.append("x")
    return 1


def by_pop(bag: Bag) -> int:
    bag.items.pop()
    return 1


def main() -> int:
    return by_clear(Bag()) + by_append(Bag()) + by_pop(Bag()) + Bag().empty()
