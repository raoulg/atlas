class Section:
    def show(self, heading: str) -> None:
        print(heading)


class Boxed(Section):
    def show(self, heading: str) -> None:
        if not heading:
            raise ValueError("a box needs a heading")
        print("[", heading, "]")
