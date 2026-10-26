"""Ledgers and boxes over a type their subclasses fill in, in both forms of a type parameter."""

from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import Generic, TypeVar

from reading.catalogs import Entry

T = TypeVar("T")


@dataclass(frozen=True)
class Version[R]:
    record: R
    number: int


class Ledger[R](ABC):
    @abstractmethod
    def from_row(self, row: tuple[str, int]) -> Version[R]: ...


class EntryLedger(Ledger[Entry]):
    def from_row(self, row: tuple[str, int]) -> Version[Entry]:
        return Version(Entry(row[0]), row[1])


class CountLedger(Ledger[Entry]):
    def from_row(self, row: tuple[str, int]) -> Version[int]:
        return Version(row[1], row[1])


class Box(ABC, Generic[T]):
    @abstractmethod
    def fill(self, items: list[T]) -> None: ...


class Numbers(Box[int]):
    def fill(self, items: list[str]) -> None:
        print(len(items))
