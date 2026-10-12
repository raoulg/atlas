"""A Flask app: a route by the methods it is handed, or by the method it is named for."""

from typing import Callable

from flask import Flask

app = Flask(__name__)


@app.route("/notes", methods=["GET", "POST"])
def notes() -> str:
    return "notes"


@app.route("/")
def index() -> str:
    return "index"


@app.post("/reset")
def reset() -> str:
    return "reset"


class Cache:
    """The project's own `get` registers nothing, whatever it is called."""

    def get(self, key: str) -> Callable[[Callable[[], str]], Callable[[], str]]:
        return lambda fn: fn


cache = Cache()


@cache.get("/cached")
def cached() -> str:
    return "cached"
