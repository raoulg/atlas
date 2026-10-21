class Source:
    def read(self) -> bytes:
        return b""


class Buffered(Source):
    pass


class Store:
    pass


class Archive(Store):
    def read(self) -> str:
        return "archive"


class Feed(Buffered, Archive):
    def read(self) -> str:
        return "feed"


class Tape(Buffered, Archive):
    def read(self) -> bytes:
        return b"tape"
