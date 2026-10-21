"""Each step of a job leaves something a reader sees: a place, a value it is handed, the end of the run."""

import sys
from abc import ABC, abstractmethod
from collections.abc import Callable
from pathlib import Path

RUNS = 0


class Task(ABC):
    @abstractmethod
    def run(self) -> None: ...


class Echo(Task):
    def run(self) -> None:
        print("echo")


def run_task(task: Task) -> None:
    task.run()


def finish(on_done: Callable[[], None]) -> None:
    on_done()


def report(message: str) -> None:
    print(message, file=sys.stderr)


def count() -> None:
    global RUNS
    RUNS += 1


async def settle(log: Path) -> None:
    log.write_text("settled\n")


def stop(reason: str) -> None:
    sys.exit(reason)


async def main(log: Path) -> int:
    run_task(Echo())
    finish(lambda: print("done"))
    report("ran")
    count()
    await settle(log)
    stop("finished")
    return RUNS
