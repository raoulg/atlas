class Block:
    def show(self, heading: str) -> None:
        print(heading)


class Section(Block):
    """A block that opens a chapter: it shows as any block does."""


class Boxed(Section):
    def show(self, heading: str) -> None:
        if not heading:
            raise ValueError("a box needs a heading")
        print("[", heading, "]")
