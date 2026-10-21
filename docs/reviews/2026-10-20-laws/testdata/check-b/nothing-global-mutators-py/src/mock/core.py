SEEN: dict[str, int] = {}
NAMES: list[str] = []


def forget_all() -> None:
    SEEN.clear()


def merge(d: dict[str, int]) -> None:
    SEEN.update(d)


def add_name(n: str) -> None:
    NAMES.append(n)


def flip() -> None:
    NAMES.reverse()


def main() -> int:
    forget_all()
    merge({"a": 1})
    add_name("x")
    flip()
    return len(SEEN) + len(NAMES)
