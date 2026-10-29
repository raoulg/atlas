"""The settings of a project, in its own settings file or in its manifest, and its lock."""

from pathlib import Path


def text(root: Path, name: str) -> str:
    """The text of the file `name` in the project at `root`."""
    with open(root / name) as lines:
        return lines.read()


def settings(root: Path) -> str:
    """The project's settings: what its `knot.toml` says, else its `pyproject.toml`."""
    return text(root, "knot.toml") or text(root, "pyproject.toml")


def lock(root: Path) -> str:
    """The versions the project is locked to."""
    return text(root, "knot.lock")
