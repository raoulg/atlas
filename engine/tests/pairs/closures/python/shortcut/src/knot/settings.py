"""The settings of a project, in its own settings file or in its manifest, and its lock."""

from pathlib import Path


def settings(root: Path) -> str:
    """The project's settings: what its `knot.toml` says, else its `pyproject.toml`."""
    text = lambda name: open(root / name).read()
    return text("knot.toml") or text("pyproject.toml")


def lock(root: Path) -> str:
    """The versions the project is locked to."""

    def text(name: str) -> str:
        with open(root / name) as lines:
            return lines.read()

    return text("knot.lock")
