from dataclasses import dataclass


@dataclass
class Order:
    item: str
    amount: float
