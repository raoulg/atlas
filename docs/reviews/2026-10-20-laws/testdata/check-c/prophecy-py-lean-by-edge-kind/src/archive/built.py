from dataclasses import dataclass

from archive.plan import Vault, seal


def calls(report: str) -> str:
    return seal(report)


def builds() -> None:
    Vault()


def member(v: str) -> None:
    Vault().keep(v)


def takes(v: Vault) -> int:
    return 1


def returns() -> Vault | None:
    return None


@dataclass
class Holds:
    vault: Vault


class Extends(Vault):
    pass
