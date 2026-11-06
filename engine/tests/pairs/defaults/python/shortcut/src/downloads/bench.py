from pathlib import Path


def made(home: Path, name: str) -> Path:
    """A file of the workbench in `home`."""
    return home / ".made" / name
