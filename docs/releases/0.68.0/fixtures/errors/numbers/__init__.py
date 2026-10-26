"""Add up the numbers in a file, one per line."""


def parse(line: str) -> int:
    if not line.strip():
        raise ValueError("an empty line")
    return int(line)


def total(path: str) -> int:
    with open(path) as f:
        return sum(parse(line) for line in f)


def main() -> None:
    print(total("numbers.txt"))
