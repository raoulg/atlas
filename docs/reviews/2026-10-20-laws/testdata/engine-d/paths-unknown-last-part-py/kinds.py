"""One route unpacks into store/{kind}/{id}, another lists store: they meet at store/."""

import sys
from pathlib import Path

STORE = Path("store")


def unpack() -> None:
    kind, ident = sys.argv[1], sys.argv[2]
    (STORE / kind / ident).write_text("payload")


def listing() -> None:
    for p in STORE.iterdir():
        print(p)


def run() -> None:
    unpack()
    listing()
