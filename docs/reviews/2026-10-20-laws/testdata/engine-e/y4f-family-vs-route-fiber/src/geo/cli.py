from geo.shapes import Circle


def unit_circle() -> Circle:
    return Circle(1.0)


def circle() -> None:
    c = unit_circle()
    print(c.area())
