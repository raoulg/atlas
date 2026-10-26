import json
from dataclasses import asdict, dataclass
from pathlib import Path


@dataclass
class Visit:
    member: str
    day: str


class Desk:
    """Writes visits to one file under root, reads member names from another."""

    def __init__(self, root: Path) -> None:
        self.root = root

    def log(self, visit: Visit) -> None:
        (self.root / "visits.json").write_text(json.dumps(asdict(visit)))

    def members(self) -> list[str]:
        return json.loads((self.root / "members.json").read_text())

    def log_named(self, visit: Visit, out: Path) -> None:
        out.write_text(json.dumps(asdict(visit)))

    def members_named(self, src: Path) -> list[str]:
        return json.loads(src.read_text())
