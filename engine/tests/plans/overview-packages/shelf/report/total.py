"""A total the report prints beside the titles."""

from shelf.store.disk import Shelf, count


def total(shelf: Shelf) -> None:
    print(count(shelf))
