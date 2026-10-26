from units.core import Length

__all__ = ["Square"]


class Shown:
    def show(self) -> str:
        return type(self).__name__


class Square(Shown):
    def __init__(self, side: Length) -> None:
        self.side = side

    def area(self) -> float:
        return self.side.meters * self.side.meters


def label(square: Square) -> str:
    return f"{square.show()} of {square.area()} m²"
