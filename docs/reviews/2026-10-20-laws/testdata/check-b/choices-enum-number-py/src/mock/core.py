import enum


class Mode(enum.Enum):
    FAST = "fast"
    SLOW = "slow"


def by_enum(mode: Mode) -> int:
    a = 1 if mode == Mode.FAST else 2
    b = 3 if mode is Mode.FAST else 4
    return a + b


def by_number(level: int) -> int:
    a = 1 if level == 2 else 2
    b = 3 if level == 2 else 4
    return a + b


def main(mode: str) -> int:
    return by_enum(Mode(mode)) + by_number(2)
