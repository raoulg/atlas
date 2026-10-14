"""Overrides of a generic base, each read with the type its subclass fills in: in both forms of a
type parameter, through a generic base between them, and two that break what they are filled in
with, one in what it gives and one in what it takes."""

from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import Generic, TypeVar

from fixture.schema import Node

T = TypeVar("T")


@dataclass(frozen=True)
class Version[R]:
    record: R
    number: int


class Ledger[R](ABC):
    @abstractmethod
    def from_row(self, row: tuple[str, int]) -> Version[R]: ...

    @abstractmethod
    def put(self, version: Version[R]) -> None: ...


class NodeLedger(Ledger[Node]):
    def from_row(self, row: tuple[str, int]) -> Version[Node]:
        return Version(Node(id=row[0]), row[1])

    def put(self, version: Version[Node]) -> None:
        print(version.record.id)


class History[S](Ledger[list[S]]):
    """A ledger of lists; its subclasses say of what."""


class NodeHistory(History[Node]):
    def from_row(self, row: tuple[str, int]) -> Version[list[Node]]:
        return Version([Node(id=row[0])], row[1])

    def put(self, version: Version[list[Node]]) -> None:
        print(len(version.record))


class CountLedger(Ledger[Node]):
    def from_row(self, row: tuple[str, int]) -> Version[int]:
        return Version(row[1], row[1])

    def put(self, version: Version[Node]) -> None:
        print(version.number)


class Box(ABC, Generic[T]):
    @abstractmethod
    def take(self) -> list[T]: ...

    @abstractmethod
    def fill(self, items: list[T]) -> None: ...


class Numbers(Box[int]):
    def take(self) -> list[int]:
        return [1]

    def fill(self, items: list[str]) -> None:
        print(len(items))
