"""An index of entries, and a catalog that says what the index promises, at times in other words."""

from dataclasses import dataclass
from typing import Annotated, List, Optional, Sequence


@dataclass
class Entry:
    key: str


class Index:
    def entries(self) -> list[Entry]:
        return []

    def roots(self) -> Sequence[Entry]:
        return []

    def find(self, key: str) -> Optional[Entry]:
        return None

    def title(self) -> str:
        return ""

    def copy(self) -> "Index":
        return Index()

    def head(self) -> list[Entry]:
        return []


class Catalog(Index):
    def entries(self) -> List[Entry]:
        return []

    def roots(self) -> list[Entry]:
        return []

    def find(self, key: str) -> Entry | None:
        return None

    def title(self) -> Annotated[str, "what the catalog is called"]:
        return ""

    def copy(self) -> "Catalog":
        return Catalog()

    def head(self) -> list[Entry]:
        return self.entries()[:1]
