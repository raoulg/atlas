"""What the club keeps in files: a log of visits, its roster, its board, and visits on tape."""

import json
from dataclasses import asdict
from pathlib import Path
from typing import IO

from club.model import Member, Notice, Post, Visit


class Log:
    """An end product: each visit is appended to the JSON list in the file; nothing gives them back."""

    def __init__(self, path: Path) -> None:
        self.path = path

    def add(self, visit: Visit) -> None:
        rows = json.loads(self.path.read_text())
        rows.append(asdict(visit))
        self.path.write_text(json.dumps(rows))


class Roster:
    """The members, kept in a file: `load` and `reread` give back a Roster that holds them."""

    def __init__(self, path: Path, members: list[Member]) -> None:
        self.path = path
        self.members = members

    def add(self, who: Member) -> None:
        self.path.write_text(json.dumps([asdict(m) for m in [*self.members, who]]))

    def reread(self) -> "Roster":
        return Roster(self.path, [Member(**m) for m in json.loads(self.path.read_text())])

    @classmethod
    def load(cls, path: Path) -> "Roster":
        return cls(path, [Member(**m) for m in json.loads(path.read_text())])


class Board:
    """Pins a Notice and gives back the Posts: a Notice is a Post."""

    def __init__(self, path: Path) -> None:
        self.path = path

    def pin(self, notice: Notice) -> None:
        self.path.write_text(json.dumps(asdict(notice)))

    def posts(self) -> list[Post]:
        return [Notice(**json.loads(self.path.read_text()))]


class Tape:
    """Records a visit to the stream it is handed and reads names from another: no file of its own."""

    def record(self, visit: Visit, out: IO[str]) -> None:
        json.dump(asdict(visit), out)

    def names(self, src: IO[str]) -> list[str]:
        return json.load(src)
