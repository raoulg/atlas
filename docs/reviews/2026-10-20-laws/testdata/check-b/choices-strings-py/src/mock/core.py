def by_string(mode: str) -> int:
    a = 1 if mode == "fast" else 2
    b = 3 if mode == "fast" else 4
    return a + b


def by_membership(mode: str) -> int:
    a = 1 if mode in {"fast", "quick"} else 2
    b = 3 if mode == "fast" else 4
    return a + b


def by_match(mode: str) -> int:
    match mode:
        case "fast":
            a = 1
        case _:
            a = 2
    return a + (3 if mode == "fast" else 4)


def main(mode: str) -> int:
    return by_string(mode) + by_membership(mode) + by_match(mode)
