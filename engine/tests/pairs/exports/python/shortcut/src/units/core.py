def scale(x: float, k: float) -> float:
    return x * k


def metres(text: str) -> float:
    return float(text.removesuffix("m"))
