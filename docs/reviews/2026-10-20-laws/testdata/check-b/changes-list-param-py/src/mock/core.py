def add_one(xs: list[int]) -> None:
    xs.append(1)


def put(d: dict[str, int], k: str) -> None:
    d[k] = 1


def collect() -> list[int]:
    xs: list[int] = []
    add_one(xs)
    d: dict[str, int] = {}
    put(d, "a")
    return xs
