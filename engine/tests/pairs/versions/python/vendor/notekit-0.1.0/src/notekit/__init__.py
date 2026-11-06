from pathlib import Path


def save(path: str, text: str) -> None:
    """Writes `text` to `path`."""
    Path(path).write_text(text)
