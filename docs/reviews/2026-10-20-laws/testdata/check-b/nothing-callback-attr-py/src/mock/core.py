from collections.abc import Callable


class Job:
    def __init__(self, on_done: Callable[[], None]) -> None:
        self.on_done = on_done

    def reset(self) -> None:
        self.on_done = lambda: None

    def finish(self) -> None:
        self.on_done()


def run(on_done: Callable[[], None]) -> None:
    on_done()


def main() -> int:
    job = Job(lambda: print("done"))
    job.finish()
    job.reset()
    run(lambda: print("ran"))
    return 0
