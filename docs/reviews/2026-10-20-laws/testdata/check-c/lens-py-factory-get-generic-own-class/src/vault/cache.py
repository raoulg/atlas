import json
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Generic, TypeVar

T = TypeVar("T")


@dataclass
class Table:
    rows: list[str]


class Cache(Generic[T]):
    """As lens-py-factory-get-returns-own-class, with the class generic: `load` gives a Cache[T]."""

    def __init__(self, path: Path, table: Table, tag: T) -> None:
        self.path = path
        self.table = table
        self.tag = tag

    def save(self, table: Table) -> None:
        self.path.write_text(json.dumps(asdict(table)))

    @classmethod
    def load(cls, path: Path, tag: T) -> "Cache[T]":
        return cls(path, Table(**json.loads(path.read_text())), tag)
