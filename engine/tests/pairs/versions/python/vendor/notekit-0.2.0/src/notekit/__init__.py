from pathlib import Path


def save(path: str, text: str) -> None:
    """Writes `text` beside `path` and renames it into place, so that a reader never finds half of it."""
    partial = Path(f"{path}.part")
    partial.write_text(text)
    partial.rename(path)
