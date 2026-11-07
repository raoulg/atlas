"""The command: list the folder of packages itself, and print what each package it lists says."""

from pathlib import Path

from shelf.packages import Packages

ROOT = Path("site")


def installed() -> list[str]:
    return [p.name for p in (ROOT / "packages").iterdir()]


def main() -> None:
    shelf = Packages(ROOT)
    print(shelf.settings())
    for name in installed():
        print(shelf.manifest(name), shelf.lock(name))
