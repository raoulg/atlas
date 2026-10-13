from zoo_keeper import Animal


class Meal:
    def __init__(self, animal: Animal) -> None:
        self.animal = animal


def feed(animal: Animal) -> Meal:
    return Meal(animal)
