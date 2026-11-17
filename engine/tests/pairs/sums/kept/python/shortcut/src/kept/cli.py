"""The attachments of a knowledge base, each kept in a file of its own or in a folder with an index: where one is
kept is a string that says the kind before its path, split again where it is read."""

from pathlib import Path


def lees(plek: str) -> str:
    soort, pad = plek.split(":", 1)
    if soort == "map":
        return (Path(pad) / "index.txt").read_text(encoding="utf-8")
    return Path(pad).read_text(encoding="utf-8")


def main() -> None:
    for plek in (f"bestand:{'bijlagen/bron.txt'}", f"map:{'bijlagen/los'}"):
        print(lees(plek))
