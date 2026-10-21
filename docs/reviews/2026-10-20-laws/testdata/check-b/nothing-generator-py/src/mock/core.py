import time
from collections.abc import Iterator
from contextlib import contextmanager


@contextmanager
def timed(label: str) -> Iterator[None]:
    start = time.perf_counter()
    yield
    print(label, time.perf_counter() - start)


def ticks(n: int) -> Iterator[None]:
    for _ in range(n):
        yield


def main() -> int:
    with timed("x"):
        pass
    for _ in ticks(2):
        pass
    return 0
