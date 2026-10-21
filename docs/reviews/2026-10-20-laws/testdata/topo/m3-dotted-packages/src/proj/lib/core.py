from dataclasses import dataclass


@dataclass
class Rec:
    name: str


def make(name: str) -> Rec:
    return Rec(name)


def ping() -> None:
    print("ping")
