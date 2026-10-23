"""How long a run may take in its mode: one table of plans, a Plan by its Mode."""

from dataclasses import dataclass
from enum import Enum


class Mode(Enum):
    SERIAL = "serial"
    PARALLEL = "parallel"


@dataclass(frozen=True)
class Plan:
    workers: int
    timeout: int


def timeout(plans: dict[Mode, Plan], mode: Mode) -> int:
    return plans[mode].timeout
