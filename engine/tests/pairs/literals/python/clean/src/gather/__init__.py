"""A tree's items, gathered into one list that `_collect_into` fills.

`by_literal` and `by_call` make the empty list each in its own way, `[]` and `list()`: the same
list, made by the body.
"""

from dataclasses import dataclass


@dataclass
class Item:
    """An item of a node."""

    n: int


@dataclass
class Node:
    """A node of the tree."""

    n: int
    kids: list["Node"]


def _collect_into(node: Node, out: list[Item]) -> None:
    out.append(Item(node.n))
    for k in node.kids:
        _collect_into(k, out)


def by_literal(node: Node) -> list[Item]:
    """The items of the tree, gathered into a list written as a literal."""
    items: list[Item] = []
    _collect_into(node, items)
    return items


def by_call(node: Node) -> list[Item]:
    """The items of the tree, gathered into a list a call makes."""
    items: list[Item] = list()
    _collect_into(node, items)
    return items
