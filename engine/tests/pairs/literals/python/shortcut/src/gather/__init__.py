"""A tree's items, gathered into one list that `_collect_into` fills.

`by_constant` starts from the module's list, `by_call` makes its own with `list()`.
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


SEED: list[Item] = []


def _collect_into(node: Node, out: list[Item]) -> None:
    out.append(Item(node.n))
    for k in node.kids:
        _collect_into(k, out)


def by_constant(node: Node) -> list[Item]:
    """The items of the tree, gathered into the module's list."""
    items = SEED
    _collect_into(node, items)
    return items


def by_call(node: Node) -> list[Item]:
    """The items of the tree, gathered into a list a call makes."""
    items: list[Item] = list()
    _collect_into(node, items)
    return items
