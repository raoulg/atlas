from typing import overload

__all__ = ["Length", "log", "scale"]


class Length:
    def __init__(self, meters: float) -> None:
        self._meters = meters

    @property
    def meters(self) -> float:
        return self._meters

    @meters.setter
    def meters(self, value: float) -> None:
        self._meters = value

    def __add__(self, other: "Length") -> "Length":
        return Length(self._meters + other.meters)

    def __eq__(self, other: object) -> bool:
        return isinstance(other, Length) and other.meters == self._meters


@overload
def scale(x: int, k: int) -> int: ...
@overload
def scale(x: float, k: float) -> float: ...
def scale(x, k):
    return x * k


def log(*parts: str, **fields: object) -> None:
    print(*parts, fields)
