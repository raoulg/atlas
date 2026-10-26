"""Read seven files from each folder below one directory: past a column's worth, they fold into it."""

from pathlib import Path

ROOT = Path("data")


def run() -> None:
    for m in ROOT.glob("*/a.xml"):
        print(m.read_text())
    for m in ROOT.glob("*/b.xml"):
        print(m.read_text())
    for m in ROOT.glob("*/c.xml"):
        print(m.read_text())
    for m in ROOT.glob("*/d.xml"):
        print(m.read_text())
    for m in ROOT.glob("*/e.xml"):
        print(m.read_text())
    for m in ROOT.glob("*/f.xml"):
        print(m.read_text())
    for m in ROOT.glob("*/g.xml"):
        print(m.read_text())
