import functools


def log(msg: str):
    print(msg)


@functools.cache
def cached():
    return 2


def fact(n: int):
    return 1 if n <= 1 else n * fact(n - 1)


def main() -> int:
    r = log("x")
    fact(3)
    return cached() + (0 if r is None else 1)
