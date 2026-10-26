def score(x: int):
    return -x


def ranked(xs: list[int]) -> list[int]:
    return sorted(xs, key=score)


def main() -> int:
    return len(ranked([3, 1]))
