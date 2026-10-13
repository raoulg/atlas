from aardvark import Meal
from zoo_keeper import Animal
from zoo_keeper.keep import keep


def keep_many(count: int) -> list[Meal]:
    return [keep(Animal("lion")) for _ in range(count)]
