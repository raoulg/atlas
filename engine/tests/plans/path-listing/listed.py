"""List a directory and read the newest file in each folder it lists: `iterdir()` lists what
`glob("*")` lists, a sort key is handed the paths it orders, and a key no code decides picks one of
the values a mapping the code writes holds."""

import os
from pathlib import Path

ASSETS = Path("assets")
KINDS = {"a": "regelingen", "b": "ontwerp_regelingen"}


def run() -> None:
    for m in (ASSETS / KINDS[os.environ["KIND"]]).iterdir():
        print(max(m.glob("*.xml"), key=lambda p: p.stat().st_mtime).read_text())
