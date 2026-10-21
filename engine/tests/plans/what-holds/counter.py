from dataclasses import dataclass
from pathlib import Path

__all__ = ["bump", "pick", "draw_all"]


@dataclass
class Table:
    rows: list[str]


class Shape:
    def draw(self) -> str:
        raise NotImplementedError


class Circle(Shape):
    def draw(self) -> str:
        return "o"


class Square(Shape):
    def draw(self) -> str:
        return "[]"


def read_count(path: Path) -> int:
    return int(path.read_text())


def write_count(path: Path, n: int) -> None:
    path.write_text(str(n))


def bump(path: Path) -> None:
    n = read_count(path)
    write_count(path, n + 1)


def load_csv(path: Path) -> Table:
    return Table(path.read_text().splitlines())


def load_json(path: Path) -> Table:
    return Table([path.read_text()])


def show(t: Table) -> None:
    print(t.rows)


def pick(path: Path, raw: bool) -> None:
    if raw:
        t = load_csv(path)
    else:
        t = load_json(path)
    show(t)


def draw_all(shapes: list[Shape]) -> None:
    for s in shapes:
        print(s.draw())
