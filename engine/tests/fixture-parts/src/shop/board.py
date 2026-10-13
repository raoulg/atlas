"""Prefixes as the frameworks give them: a blueprint's own, one its app puts in its place, a
Litestar router's path and a controller's."""

from flask import Blueprint, Flask
from litestar import Controller, Router, get

board = Blueprint("board", __name__, url_prefix="/board")
moved = Blueprint("moved", __name__, url_prefix="/old")


@board.get("/pins")
def pins() -> str:
    return "pins"


@moved.get("/notes")
def moved_notes() -> str:
    return "notes"


site = Flask(__name__)
site.register_blueprint(board)
site.register_blueprint(moved, url_prefix="/new")


@get("/levels")
def levels() -> dict:
    return {}


class Shelves(Controller):
    path = "/shelves"

    @get("/{shelf:int}")
    def shelf(self, shelf: int) -> dict:
        return {}


v2 = Router(path="/v2", route_handlers=[levels, Shelves])
