"""A knowledge base kept in a file, a node per line; read back as the graph of its nodes."""

from dataclasses import dataclass
from pathlib import Path


@dataclass
class Begrip:
    uuid: str
    naam: str


@dataclass
class Bron:
    uuid: str
    url: str


Node = Begrip | Bron


@dataclass
class Graaf:
    nodes: list[Node]


class Kennisbank:
    def __init__(self, path: Path) -> None:
        self.path = path

    def voeg_toe(self, bron: Bron) -> None:
        self._write(f"{bron.uuid} bron {bron.url}")

    def graaf(self) -> Graaf:
        return Graaf([self._node(line) for line in self.path.read_text().splitlines()])

    def _node(self, line: str) -> Node:
        uuid, soort, tekst = line.split(" ", 2)
        return Bron(uuid, tekst) if soort == "bron" else Begrip(uuid, tekst)

    def _write(self, line: str) -> None:
        self.path.write_text(self.path.read_text() + line + "\n")
