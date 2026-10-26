TOTAL = 0


class Counter:
    total = 0


def bump_global() -> None:
    global TOTAL
    TOTAL += 1


def bump_class() -> None:
    Counter.total += 1


def main() -> int:
    bump_global()
    bump_class()
    return TOTAL + Counter.total
