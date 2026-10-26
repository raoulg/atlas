"""The same chart, which binds its title and its size to names before it hands them on."""


class Size:
    def __init__(self, width: int) -> None:
        self.width = width


class Chart:
    def __init__(self, title: str, size: Size) -> None:
        self.title = title
        self.size = size

    def take(self, title: str, width: int) -> None:
        self.title = title
        self.size.width = width

    def lend(self, other: "Chart") -> None:
        t = self.title
        s = self.size
        other.take(t, s.width)
