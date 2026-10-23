"""How long a run may take in its mode: the modes in one list, their plans in another at the same places."""

from dataclasses import dataclass
from enum import Enum


class Mode(Enum):
    SERIAL = "serial"
    PARALLEL = "parallel"


@dataclass(frozen=True)
class Plan:
    workers: int
    timeout: int


def index(modes: list[Mode], mode: Mode) -> int:
    return modes.index(mode)


def timeout(modes: list[Mode], plans: list[Plan], mode: Mode) -> int:
    return plans[index(modes, mode)].timeout
