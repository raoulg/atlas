"""What the club keeps in files: a log of visits, its roster, its board, and visits on tape."""

import json
from dataclasses import asdict
from pathlib import Path

from club.model import Member, Notice, Visit


class Log:
    """Appends each visit to the JSON list in the file and gives the list back as it is kept."""

    def __init__(self, path: Path) -> None:
        self.path = path

    def add(self, visit: Visit) -> None:
        rows = json.loads(self.path.read_text())
        rows.append(asdict(visit))
        self.path.write_text(json.dumps(rows))

    def rows(self) -> list[dict]:
        return json.loads(self.path.read_text())


class Roster:
    """The members, kept in a file: `load` gives back a Roster that holds them as rows."""

    def __init__(self, path: Path, rows: list[dict]) -> None:
        self.path = path
        self.rows = rows

    def add(self, who: Member) -> None:
        self.path.write_text(json.dumps([*self.rows, asdict(who)]))

    @classmethod
    def load(cls, path: Path) -> "Roster":
        return cls(path, json.loads(path.read_text()))


class Board:
    """Pins a Notice and gives back what the file holds."""

    def __init__(self, path: Path) -> None:
        self.path = path

    def pin(self, notice: Notice) -> None:
        self.path.write_text(json.dumps(asdict(notice)))

    def posts(self) -> list[dict]:
        return [json.loads(self.path.read_text())]


class Tape:
    """Records a visit to its file and reads names back from it."""

    def __init__(self, path: Path) -> None:
        self.path = path

    def record(self, visit: Visit) -> None:
        self.path.write_text(json.dumps(asdict(visit)))

    def names(self) -> list[str]:
        return json.loads(self.path.read_text())
