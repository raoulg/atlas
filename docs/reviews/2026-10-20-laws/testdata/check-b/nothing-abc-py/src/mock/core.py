from abc import ABC, abstractmethod


class Step(ABC):
    @abstractmethod
    def run(self) -> None: ...


class Show(Step):
    def run(self) -> None:
        print("shown")


def run_step(s: Step) -> None:
    s.run()


def main() -> int:
    run_step(Show())
    return 0
