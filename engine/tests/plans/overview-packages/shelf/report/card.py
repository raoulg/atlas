"""A card of the shelf: the report makes it and gives it back, and no function of the shelf takes it."""

from dataclasses import dataclass

from shelf.store.disk import Shelf


@dataclass
class Card:
    first: str
    books: int


def card(shelf: Shelf) -> Card:
    return Card(shelf.rows[0].title, len(shelf.rows))
