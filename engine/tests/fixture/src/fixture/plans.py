"""A design kept in the tree: sketches that are not built yet."""

from typing import Callable, TypeVar

T = TypeVar("T")


def prophecy(**_: str) -> Callable[[T], T]:
    """Marks sketch code that is not built yet."""

    def mark(item: T) -> T:
        return item

    return mark


def current(text: str) -> str:
    return text.strip()


@prophecy(supersedes="fixture.plans.current")
def tidy(text: str) -> str: ...


@prophecy(supersedes="fixture.plans.gone")
def forgotten(text: str) -> str: ...


def report(text: str) -> str:
    return tidy(text)


def keep(text: str) -> None:
    text.strip()


def save(text: str) -> None:
    keep(text)


@prophecy(supersedes="fixture.plans.keep")
def stash(text: str) -> None: ...
