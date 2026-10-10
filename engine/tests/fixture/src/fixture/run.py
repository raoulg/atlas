"""Work handed to an executor or a thread is still called."""

from concurrent.futures import ThreadPoolExecutor
from threading import Thread


def work(n: int) -> int:
    return n * 2


def done() -> None:
    print("done")


def run_all(ns: list[int]) -> None:
    with ThreadPoolExecutor() as pool:
        for n in ns:
            pool.submit(work, n)
    Thread(target=done).start()
