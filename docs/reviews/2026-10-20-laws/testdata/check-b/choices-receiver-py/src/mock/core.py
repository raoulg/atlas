class Runner:
    def __init__(self, mode: str) -> None:
        self.mode = mode

    def size(self) -> int:
        a = 64 if self.mode == "parallel" else 1
        return a + (8 if self.mode == "parallel" else 1)


def size(mode: str) -> int:
    a = 64 if mode == "parallel" else 1
    return a + (8 if mode == "parallel" else 1)


def main(mode: str) -> int:
    return Runner(mode).size() + size(mode)
