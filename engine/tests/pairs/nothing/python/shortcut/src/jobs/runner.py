"""The same job runner, with what its steps leave out of sight of a reader."""

import logging
from collections.abc import Callable
from typing import Protocol

logger = logging.getLogger(__name__)


class Task(Protocol):
    def run(self) -> None: ...


class Echo:
    def run(self) -> None:
        print("echo")


def run_task(task: Task) -> None:
    task.run()


class Job:
    def __init__(self, on_done: Callable[[], None]) -> None:
        self.on_done = on_done

    def finish(self) -> None:
        self.on_done()


def report(message: str) -> None:
    logger.info(message)


class Counter:
    runs = 0


def count() -> None:
    Counter.runs += 1


async def settle() -> None:
    pass


def stop(reason: str) -> None:
    raise SystemExit(reason)


async def main() -> int:
    run_task(Echo())
    Job(lambda: print("done")).finish()
    report("ran")
    count()
    await settle()
    stop("finished")
    return Counter.runs
