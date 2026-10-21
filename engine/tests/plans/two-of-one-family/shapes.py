from dataclasses import dataclass

__all__ = ["report", "total"]


@dataclass
class Shape:
    name: str


@dataclass
class Circle(Shape):
    r: float


@dataclass
class Square(Shape):
    s: float


def total(shapes: list[Shape]) -> int:
    return len(shapes)


def show_circle(c: Circle) -> None:
    print(c.r)


def show_square(q: Square) -> None:
    print(q.s)


def report(c: Circle, q: Square) -> None:
    show_circle(c)
    show_square(q)
