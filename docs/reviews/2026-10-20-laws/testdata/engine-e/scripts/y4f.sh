# claim 4: the family quotient against a route fiber. The route `circle` only builds and prints a Circle.
# A: another route, `total`, takes Shape as a type (so Shape roots a family {Shape, Circle, Square}).
# B: the route `total` is gone (no code outside `circle`'s fiber names Shape as a type); `circle` is unchanged.
source ${0:a:h}/base.sh
R=$E/y4f-family-vs-route-fiber; $T/mkrepo.sh $R; mkdir -p $R/src/geo
cat > $R/pyproject.toml <<'P'
[project]
name = "geo"
version = "0.1.0"
requires-python = ">=3.12"

[project.scripts]
circle = "geo.cli:circle"
total = "geo.cli:total"
P
printf 'from geo.cli import circle, total\n\n__all__ = ["circle", "total"]\n' > $R/src/geo/__init__.py
cat > $R/src/geo/shapes.py <<'P'
class Shape:
    def area(self) -> float:
        return 0.0


class Circle(Shape):
    def __init__(self, r: float) -> None:
        self.r = r

    def area(self) -> float:
        return 3.14159 * self.r * self.r


class Square(Shape):
    def __init__(self, side: float) -> None:
        self.side = side

    def area(self) -> float:
        return self.side * self.side
P
cat > $R/src/geo/cli.py <<'P'
from geo.shapes import Circle, Shape, Square


def unit_circle() -> Circle:
    return Circle(1.0)


def summed(shapes: list[Shape]) -> float:
    return sum(s.area() for s in shapes)


def circle() -> None:
    c = unit_circle()
    print(c.area())


def total() -> None:
    print(summed([Circle(1.0), Square(2.0)]))
P
$T/commit.sh $R "A: circle and total" >/dev/null
cat > $R/src/geo/cli.py <<'P'
from geo.shapes import Circle


def unit_circle() -> Circle:
    return Circle(1.0)


def circle() -> None:
    c = unit_circle()
    print(c.area())
P
printf 'from geo.cli import circle\n\n__all__ = ["circle"]\n' > $R/src/geo/__init__.py
sed -i '' '/^total = /d' $R/pyproject.toml
$T/commit.sh $R "B: total is gone" >/dev/null
