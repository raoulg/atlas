from aardvark import Meal
from aardvark.feed import feed
from zoo_keeper import Animal


def keep(animal: Animal) -> Meal:
    return feed(animal)
