import json
from dataclasses import asdict, dataclass
from pathlib import Path


@dataclass
class Visit:
    member: str
    day: str


class Log:
    """An end product: visits are appended to a JSON list; nothing here gives them back."""

    def __init__(self, path: Path) -> None:
        self.path = path

    def add(self, visit: Visit) -> None:
        rows = json.loads(self.path.read_text())
        rows.append(asdict(visit))
        self.path.write_text(json.dumps(rows))
