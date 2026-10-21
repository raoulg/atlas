from typing import Protocol


class Task(Protocol):
    def run(self) -> None: ...


class Echo:
    def run(self) -> None:
        print("echo")


def run_task(t: Task) -> None:
    t.run()


def main() -> int:
    run_task(Echo())
    return 0
