"""A chart lends another its title and its width, handing each as a field of its own."""


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
        other.take(self.title, self.size.width)
