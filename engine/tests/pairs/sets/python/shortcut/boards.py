"""The same boards, where the lending board reaches into the other's field and sets its pen there."""

from dataclasses import dataclass


@dataclass
class Pen:
    colour: str


class Board:
    def __init__(self, pen: Pen) -> None:
        self.pen = pen

    def lend(self, other: "Board") -> None:
        other.pen = self.pen
