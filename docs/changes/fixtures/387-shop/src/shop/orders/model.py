"""Orders, as frames of lines."""


class Frame:
    def __init__(self, rows: list[str]) -> None:
        self.rows = rows

    def first(self) -> str:
        return self.rows[0]


class Kept(Frame):
    def __init__(self, rows: list[str]) -> None:
        super().__init__(rows)


class Copied(Frame):
    def __init__(self, rows: list[str]) -> None:
        super().__init__(rows)


class Missing:
    pass


def find(frames: list[Frame], at: int) -> Frame | Missing:
    return frames[at] if at < len(frames) else Missing()
