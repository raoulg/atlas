class Runner:
    def __init__(self, mode: str) -> None:
        self.mode = mode

    def batch(self) -> int:
        return 64 if self.mode == "parallel" else 1

    def workers(self) -> int:
        return 8 if self.mode == "parallel" else 1


def batch(mode: str) -> int:
    return 64 if mode == "parallel" else 1


def workers(mode: str) -> int:
    return 8 if mode == "parallel" else 1


def main(mode: str) -> int:
    r = Runner(mode)
    return r.batch() + r.workers() + batch(mode) + workers(mode)
