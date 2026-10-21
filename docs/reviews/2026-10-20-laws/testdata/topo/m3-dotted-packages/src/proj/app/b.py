from proj.lib.core import Rec, make


def show(r: Rec) -> None:
    print(r.name)


def show_one(name: str) -> None:
    show(make(name))
