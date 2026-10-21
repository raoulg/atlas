from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from farm.barn import Barn


@dataclass
class Cow:
    litres: int

    def give(self, barn: Barn) -> int:
        return self.litres + len(barn.cows)
