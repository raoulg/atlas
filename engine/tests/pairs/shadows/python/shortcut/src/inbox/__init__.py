"""Letters sorted onto a pile, the locals named like the module's functions."""

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

    def tidy(s: str) -> str:
        return s.strip().lower()

    addressed: list[Letter] = []
    returned: list[Letter] = []
    for letter in incoming:
        if tidy(letter.to) == tidy(name):
            addressed.append(letter)
        else:
            returned.append(letter)
    size = len(addressed)
    pile.letters.extend(addressed)
    pile.returned.extend(returned)
    pile.counts.append(size)
