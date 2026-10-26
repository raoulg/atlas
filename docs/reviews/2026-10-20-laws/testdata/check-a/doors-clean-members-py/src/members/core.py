from dataclasses import dataclass
from typing import Self


@dataclass
class Point:
    x: float
    y: float


class Registry:
    def __init__(self, names: list[str]):
        self.names = names

    @classmethod
    def empty(cls) -> Self:
        return cls([])

    @staticmethod
    def normal(name: str) -> str:
        return name.lower()

    @property
    def size(self) -> int:
        return len(self.names)
