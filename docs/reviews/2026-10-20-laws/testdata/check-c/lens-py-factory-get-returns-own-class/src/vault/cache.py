import json
from dataclasses import asdict, dataclass
from pathlib import Path


@dataclass
class Table:
    rows: list[str]


class Cache:
    """Saves its Table to disk; `load` reads it back as a Cache that holds the Table."""

    def __init__(self, path: Path, table: Table) -> None:
        self.path = path
        self.table = table

    def save(self, table: Table) -> None:
        self.path.write_text(json.dumps(asdict(table)))

    @classmethod
    def load(cls, path: Path) -> "Cache":
        return cls(path, Table(**json.loads(path.read_text())))
