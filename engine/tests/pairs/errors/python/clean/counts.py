"""The total of the counts some lines hold: a line that holds no count is an error of the program's own class, which
each border hands on, up to the route."""


class NotACount(Exception):
    """A line that holds no whole number."""


def parse(line: str) -> int:
    if not line.isdigit():
        raise NotACount(line)
    return int(line)


def total(lines: list[str]) -> int:
    return sum(parse(line) for line in lines)


def run() -> None:
    print(total(["1", "2", "x"]))
