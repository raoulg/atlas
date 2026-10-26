class Event:
    def __init__(self, name: str) -> None:
        self.name = name


class Click(Event):
    def __init__(self, x: int) -> None:
        super().__init__("click")
        self.x = x


class Handler:
    def handle(self, e: Event) -> str:
        return e.name


class ClickHandler(Handler):
    def handle(self, e: Click) -> str:
        return f"click at {e.x}"
