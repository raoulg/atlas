class Shelter:
    def count(self) -> int:
        return 0


class Kennel(Shelter):
    def count(self) -> str:  # type: ignore
        return "none"
