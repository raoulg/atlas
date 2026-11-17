from typing import Callable, TypeVar

F = TypeVar("F")

def option(axis: str) -> Callable[[F], F]: ...
