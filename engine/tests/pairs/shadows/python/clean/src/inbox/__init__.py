"""Letters sorted onto a pile."""

from dataclasses import dataclass, field

__all__ = ["Letter", "Pile", "addressed", "total", "sort"]


@dataclass
class Letter:
    to: str


@dataclass
class Pile:
    letters: list[Letter] = field(default_factory=list)
    returned: list[Letter] = field(default_factory=list)
    counts: list[int] = field(default_factory=list)


def size(p: Pile) -> int:
    """How many letters a pile holds."""
    return len(p.letters)


def tidy(name: str) -> str:
    """A name as it is compared: trimmed, in lower case."""
    return name.strip().lower()


def addressed(incoming: list[Letter], name: str) -> int:
    """How many of the letters that came in are for `name`."""
    return sum(1 for letter in incoming if tidy(letter.to) == tidy(name))


def total(piles: list[Pile]) -> int:
    """How many letters the piles hold together."""
    return sum(map(size, piles))


def sort(pile: Pile, name: str, incoming: list[Letter]) -> None:
    """Puts the letters for `name` on the pile, sends the others back, and notes how many came."""

    def same(s: str) -> str:
        return s.strip().lower()

    mine: list[Letter] = []
    others: list[Letter] = []
    for letter in incoming:
        if same(letter.to) == same(name):
            mine.append(letter)
        else:
            others.append(letter)
    came = len(mine)
    pile.letters.extend(mine)
    pile.returned.extend(others)
    pile.counts.append(came)
