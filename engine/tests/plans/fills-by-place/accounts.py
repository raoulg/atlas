"""A call fills each parameter by where it writes the argument: by its name, in whatever order, and a literal fills none."""

from dataclasses import dataclass

__all__ = ["run"]


@dataclass
class Account:
    name: str


def log(account: Account) -> None:
    print(account.name)


def transfer(source: Account, target: Account, note: str) -> None:
    print(source.name, note)
    log(target)


def run(savings: Account, checking: Account) -> None:
    transfer(target=checking, source=savings, note="weekly")
