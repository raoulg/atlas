from dataclasses import dataclass


@dataclass
class Batch:
    rows: list[list[float]]


@dataclass
class Summary:
    values: list[float]
