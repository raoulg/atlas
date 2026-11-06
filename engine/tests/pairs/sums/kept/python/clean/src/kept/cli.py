"""The attachments of a knowledge base, each kept in a file of its own or in a folder with an index: where one is
kept is a variant, built with its path and taken apart where it is read."""

from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Bestand:
    pad: Path


@dataclass(frozen=True)
class Map:
    dir: Path


Plek = Bestand | Map


def lees(plek: Plek) -> str:
    match plek:
        case Bestand(pad):
            return pad.read_text(encoding="utf-8")
        case Map(dir=dir):
            return (dir / "index.txt").read_text(encoding="utf-8")


def main() -> None:
    for plek in (Bestand(Path("bijlagen/bron.txt")), Map(Path("bijlagen/los"))):
        print(lees(plek))
