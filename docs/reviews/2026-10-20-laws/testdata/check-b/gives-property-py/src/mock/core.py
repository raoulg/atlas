class Store:
    def helper(self):
        return 1

    @property
    def size(self):
        return 3

    def total(self) -> int:
        return self.helper() + self.size


def main() -> int:
    return Store().total()
