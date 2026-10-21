from dataclasses import dataclass
from fractions import Fraction
from pathlib import Path


@dataclass
class Entry:
    amount: Fraction


class Ledger:
    """Puts a Fraction and gets it back inside an Entry: a library type as a field."""

    def __init__(self, path: Path) -> None:
        self.path = path

    def put(self, amount: Fraction) -> None:
        self.path.write_text(str(amount))

    def get(self) -> Entry:
        return Entry(Fraction(self.path.read_text()))
