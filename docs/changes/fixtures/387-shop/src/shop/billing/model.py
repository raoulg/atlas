"""Bills, as frames of cents."""


class Frame:
    def __init__(self, cents):
        self.cents = cents

    def total(self) -> int:
        return sum(self.cents)
