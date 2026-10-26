class Sink:
    def write(self, s: str) -> None:
        print(s)


def make():
    return Sink()


def flush(s: str) -> None:
    make().write(s)


def main() -> int:
    flush("x")
    return 0
