import json
from pathlib import Path

ASSETS = Path("assets")


def documentmappen(root: Path) -> list[Path]:
    return sorted(p for p in root.iterdir() if p.is_dir())


def read_metadata(xml_path: Path) -> dict:
    metadata_path = xml_path.parent.parent / "metadata.json"
    return json.loads(metadata_path.read_text())


def lees(map_: Path) -> list[str]:
    return [p.read_text() for p in sorted(map_.glob("*.xml"))]


def main() -> None:
    for m in documentmappen(ASSETS / "regelingen"):
        for xml in m.glob("*/Regeling/Tekst.xml"):
            print(read_metadata(xml))
        for ow in m.glob("*/OW-bestanden"):
            print(lees(ow))


BACKUPS = Path("backups")


def nieuwste() -> Path:
    return sorted(BACKUPS.glob("rkg-*.tar.gz"), key=lambda p: p.stat().st_mtime)[-1]


def bekijk() -> None:
    print(nieuwste())
