"""A feed's client through the standard library: a request without data asks for what the feed holds,
one handed data sends it there, and a request built apart goes as its method says."""

from urllib.request import Request, urlopen

FEED = "https://feeds.example.org/news"


def latest() -> bytes:
    return urlopen(FEED).read()


def post(item: str) -> None:
    urlopen(FEED, data=item.encode())


def remove(item: str) -> None:
    urlopen(Request(f"{FEED}/{item}", method="DELETE"))
