import json
from dataclasses import asdict, dataclass
from pathlib import Path


@dataclass
class Table:
    rows: list[str]


class Cache:
    """Saves a Table; `reread` gives back a Cache that holds the Table it reads."""

    def __init__(self, path: Path, table: Table) -> None:
        self.path = path
        self.table = table

    def save(self, table: Table) -> None:
        self.path.write_text(json.dumps(asdict(table)))

    def reread(self) -> "Cache":
        return Cache(self.path, Table(**json.loads(self.path.read_text())))
