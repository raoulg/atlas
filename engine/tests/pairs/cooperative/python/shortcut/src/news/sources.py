from urllib.request import urlopen

from news.store import Store


class Source:
    def __init__(self, url: str) -> None:
        self.url = url

    def read(self) -> bytes:
        with urlopen(self.url) as response:
            return response.read()


class Trimmed(Source):
    def read(self) -> bytes:
        return Source.read(self).strip()


class Cached(Source):
    def __init__(self, url: str, store: Store) -> None:
        super().__init__(url)
        self.store = store

    def read(self) -> bytes:
        if self.store.has(self.url):
            return self.store.get(self.url)
        data = super().read()
        self.store.put(self.url, data)
        return data


class Feed(Trimmed, Cached):
    pass
