"""Overrides that say what their base promises in other words, and one that breaks it."""

from typing import Annotated, List, Optional, Sequence

from fixture.schema import Node


class Catalog:
    def nodes(self) -> list[Node]:
        return []

    def roots(self) -> Sequence[Node]:
        return []

    def find(self, id: str) -> Optional[Node]:
        return None

    def title(self) -> str:
        return ""

    def head(self) -> list[Node]:
        return []


class Archive(Catalog):
    def nodes(self) -> List[Node]:
        return []

    def roots(self) -> list[Node]:
        return []

    def find(self, id: str) -> Node | None:
        return None

    def title(self) -> Annotated[str, "what the archive is called"]:
        return ""

    def head(self) -> Node:
        return Node(id="")
