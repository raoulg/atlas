from typing import Callable, TypeVar

T = TypeVar("T")

__all__ = ["prophecy"]


def prophecy(**_: str) -> Callable[[T], T]:
    def mark(item: T) -> T:
        return item

    return mark
