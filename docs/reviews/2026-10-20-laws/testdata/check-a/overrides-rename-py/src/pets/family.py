from typing import Generic, Optional, TypeVar

T = TypeVar("T")


class Animal: ...


class Dog(Animal): ...


class Shelter:
    def adopt(self) -> Animal:
        return Animal()

    def find(self, name: str) -> Optional[Animal]:
        return None


class Kennel(Shelter):
    def adopt(self) -> Dog:
        return Dog()

    def find(self, key: str) -> Animal | None:
        return Dog() if key else None


class Box(Generic[T]):
    def __init__(self, item: T) -> None:
        self.item = item

    def get(self) -> T:
        return self.item


class IntBox(Box[int]):
    def get(self) -> int:
        return self.item + 1
