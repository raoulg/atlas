from urllib.request import urlopen

from news.store import Store


class Source:
    def __init__(self, url: str) -> None:
        self.url = url

    def read(self) -> bytes:
        with urlopen(self.url) as response:
            return response.read()


class Buffered(Source):
    def lines(self) -> list[bytes]:
        return self.read().splitlines()


class Cached(Source):
    def __init__(self, url: str, store: Store) -> None:
        super().__init__(url)
        self.store = store

    def read(self) -> bytes:
        return self.store.get(self.url)


class Feed(Buffered, Cached):
    def __init__(self, url: str, store: Store) -> None:
        Cached.__init__(self, url, store)

    def read(self) -> bytes:
        return Cached.read(self)
