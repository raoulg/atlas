"""Downloads kept per kind in one folder: `fetch` writes a download below it, `unpack` makes a folder for a download's
parts below it, and `kinds` lists it. `fetch` and `unpack` take the folder from the settings they are handed, and
`kinds` spells it out, so nothing in the code says the three name one folder."""

from pathlib import Path

__all__ = ["fetch", "unpack", "kinds"]


def fetch(settings: dict, kind: str, body: str) -> None:
    """A download of one kind, kept below the folder the settings name."""
    (Path(settings["raw"]) / kind / "download.json").write_text(body)


def unpack(settings: dict, kind: str) -> None:
    """A folder for the parts of a download of one kind, below the folder the settings name."""
    (Path(settings["raw"]) / kind / "parts").mkdir(parents=True, exist_ok=True)


def kinds() -> list[Path]:
    """The kinds downloaded so far: what the folder of downloads holds."""
    return sorted(Path("data/raw").iterdir())
