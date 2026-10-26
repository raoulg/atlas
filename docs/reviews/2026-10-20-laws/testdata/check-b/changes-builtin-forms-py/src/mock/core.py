import random


def iadd(xs: list[int]) -> int:
    xs += [1]
    return len(xs)


def remove_key(d: dict[str, int], k: str) -> int:
    del d[k]
    return len(d)


def shuffled(xs: list[int]) -> int:
    random.shuffle(xs)
    return xs[0]


def sort_it(xs: list[int]) -> int:
    xs.sort()
    return xs[0]


def main() -> int:
    xs = [3, 1]
    d = {"a": 1}
    return iadd(xs) + remove_key(d, "a") + shuffled(xs) + sort_it(xs)
