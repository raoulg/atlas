"""The total of the counts some lines hold: a line that holds no count raises the language's catch-all error, and the
total swallows whatever is raised, so the route never hears of it."""


def parse(line: str) -> int:
    if not line.isdigit():
        raise RuntimeError("not a count: " + line)
    return int(line)


def total(lines: list[str]) -> int:
    try:
        return sum(parse(line) for line in lines)
    except Exception:
        return 0


def run() -> None:
    print(total(["1", "2", "x"]))
