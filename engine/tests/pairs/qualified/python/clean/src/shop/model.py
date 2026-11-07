from dataclasses import dataclass


@dataclass(frozen=True)
class Order:
    reference: str
    weight: float


@dataclass(frozen=True)
class Label:
    text: str
