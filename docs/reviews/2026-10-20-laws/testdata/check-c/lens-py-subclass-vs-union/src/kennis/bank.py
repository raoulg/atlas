import json
from dataclasses import asdict, dataclass
from pathlib import Path


@dataclass
class Node:
    name: str


@dataclass
class Bron(Node):
    url: str


@dataclass
class Begrip:
    term: str


Kennis = Begrip | Bron


class OpenBank:
    """Puts a Bron, gets the Nodes back: a Bron is a Node (an open sum, by inheritance)."""

    def __init__(self, path: Path) -> None:
        self.path = path

    def add(self, bron: Bron) -> None:
        self.path.write_text(json.dumps(asdict(bron)))

    def nodes(self) -> list[Node]:
        return [Bron(**json.loads(self.path.read_text()))]


class SumBank:
    """Puts a Bron, gets the Kennis back: a Bron is a variant (a closed sum, by a union)."""

    def __init__(self, path: Path) -> None:
        self.path = path

    def add(self, bron: Bron) -> None:
        self.path.write_text(json.dumps(asdict(bron)))

    def nodes(self) -> list[Kennis]:
        return [Bron(**json.loads(self.path.read_text()))]
