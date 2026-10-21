def elif_chain(mode: str) -> int:
    if mode == "fast":
        return 1
    elif mode == "slow":
        return 2
    return 0


def comprehension(mode: str, xs: list[int]) -> int:
    ys = [x for x in xs if mode == "fast"]
    return len(ys) + (1 if mode == "fast" else 0)


def nested(mode: str) -> int:
    def inner() -> int:
        a = 1 if mode == "fast" else 2
        return a + (3 if mode == "fast" else 4)

    return inner()


def main(mode: str) -> int:
    return elif_chain(mode) + comprehension(mode, [1]) + nested(mode)
