"""A tree's items, split into a node's own and those below it.

`split` takes the node's own from `_items_of` and the rest from `_below`, which splits each child in
turn. `climb` enters a recursion of three (`_deeper`, `_onward`, `_kids`) at two of its functions.
The floor every item is raised to, and every node is held to, is the module's.
"""

from dataclasses import dataclass


@dataclass
class Item:
    """An item of a node."""

    n: int


@dataclass
class Floor:
    """The least an item counts."""

    n: int


@dataclass
class Node:
    """A node of the tree."""

    n: int
    kids: list["Node"]


FLOOR: Floor = Floor(1)


def _items_of(node: Node, floor: Floor) -> list[Item]:
    return [Item(max(node.n, floor.n))]


def _below(node: Node) -> list[Item]:
    out: list[Item] = list()
    for k in node.kids:
        a, b = split(k)
        out.extend(a)
        out.extend(b)
    return out


def split(node: Node) -> tuple[list[Item], list[Item]]:
    """The node's own items and the items below it."""
    mine = _items_of(node, FLOOR)
    rest = _below(node)
    return mine, rest


def _deeper(node: Node) -> list[Item]:
    out: list[Item] = list()
    if node.n > FLOOR.n:
        out.extend(_onward(node))
    return out


def _kids(node: Node) -> list[Item]:
    return _deeper(node)


def _onward(node: Node) -> list[Item]:
    return _kids(node)


def climb(node: Node) -> list[Item]:
    """The items over the floor, from the node and from the rest of the tree."""
    first = _deeper(node)
    second = _onward(node)
    return first + second
