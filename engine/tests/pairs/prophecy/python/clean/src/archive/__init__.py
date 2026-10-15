from typing import Callable, TypeVar

T = TypeVar("T")

__all__ = ["prophecy"]


def prophecy(**_: str) -> Callable[[T], T]:
    """Marks sketch code that is not built yet; Atlas reads the mark by its name."""

    def mark(item: T) -> T:
        return item

    return mark
