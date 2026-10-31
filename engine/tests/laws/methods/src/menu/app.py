"""The items of the menu, read and added at one path."""

from flask import Flask

app = Flask(__name__)


@app.route("/items", methods=["POST", "GET"])
def items() -> str:
    return "items"
