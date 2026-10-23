"""The same client with one function for every request: whether it asks or sends is whether it is
handed data at run time, and the method of a request built apart is a value."""

from urllib.request import Request, urlopen

FEED = "https://feeds.example.org/news"


def latest() -> bytes:
    return urlopen(FEED).read()


def call(data: bytes | None = None) -> bytes:
    return urlopen(FEED, data).read()


def remove(item: str, method: str) -> None:
    urlopen(Request(f"{FEED}/{item}", method=method))
