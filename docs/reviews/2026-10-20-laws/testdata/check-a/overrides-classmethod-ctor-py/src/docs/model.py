from typing import Self


class Doc:
    def __init__(self, text: str) -> None:
        self.text = text

    @classmethod
    def parse(cls, text: str) -> Self:
        return cls(text.strip())


class Page(Doc):
    @classmethod
    def parse(cls, data: bytes) -> Self:
        return cls(data.decode())
