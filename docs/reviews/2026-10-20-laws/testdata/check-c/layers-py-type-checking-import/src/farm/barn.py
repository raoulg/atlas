from dataclasses import dataclass

from farm.cow import Cow


@dataclass
class Barn:
    cows: list[Cow]

    def milk(self) -> int:
        return sum(c.give(self) for c in self.cows)
