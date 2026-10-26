"""Pages made from the template that ships with the package."""

from importlib.resources import files
from pathlib import Path


def page(title: str) -> str:
    template = (Path(__file__).parent / "template.html").read_text()
    return template.replace("{title}", title)


def resource_page(title: str) -> str:
    template = files("stock").joinpath("template.html").read_text()
    return template.replace("{title}", title)
