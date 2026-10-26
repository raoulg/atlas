"""A board lends the pen it draws with to another board, which takes it itself: each board sets
only its own fields."""

from dataclasses import dataclass


@dataclass
class Pen:
    colour: str


class Board:
    def __init__(self, pen: Pen) -> None:
        self.pen = pen

    def take(self, pen: Pen) -> None:
        self.pen = pen

    def lend(self, other: "Board") -> None:
        other.take(self.pen)
