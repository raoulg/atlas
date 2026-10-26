"""Build two experts in a row and a mixture of them. Each build runs its initializer, which fills in the value it builds
through its base's (`super().__init__()`), and none takes the value built before it."""


class Component:
    def __init__(self) -> None:
        print("a component")


class Expert(Component):
    def __init__(self, size: int) -> None:
        super().__init__()
        self.size = size


class Mixture(Component):
    def __init__(self, first: Expert, second: Expert) -> None:
        super().__init__()
        self.size = first.size + second.size


def run() -> None:
    small = Expert(1)
    large = Expert(2)
    mixture = Mixture(small, large)
    print(mixture.size)
