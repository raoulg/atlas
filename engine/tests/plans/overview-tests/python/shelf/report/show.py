"""The report takes the Shelf the store makes and prints it."""

from shelf.store.disk import Shelf


def render(shelf: Shelf) -> None:
    for title in shelf.titles:
        print(title)
