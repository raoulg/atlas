class Number:
    def __init__(self, value: int) -> None:
        self.value = value


def add_one(value: Number) -> Number:
    return Number(value.value + 1)


def double(value: Number) -> Number:
    return Number(value.value * 2)


def run() -> None:
    result = EXPRESSION
    print(result.value)
