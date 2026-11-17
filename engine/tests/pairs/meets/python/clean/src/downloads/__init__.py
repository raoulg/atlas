"""Downloads kept per kind in one folder: `fetch` writes a download below it, `unpack` makes a folder for a download's
parts below it, and `kinds` lists it. All three name the folder by one constant, so what `fetch` writes and what
`unpack` makes are what `kinds` lists."""

from pathlib import Path

__all__ = ["fetch", "unpack", "kinds"]

RAW = Path("data") / "raw"


def fetch(kind: str, body: str) -> None:
    """A download of one kind, kept below the folder of downloads."""
    (RAW / kind / "download.json").write_text(body)


def unpack(kind: str) -> None:
    """A folder for the parts of a download of one kind, below the folder of downloads."""
    (RAW / kind / "parts").mkdir(parents=True, exist_ok=True)


def kinds() -> list[Path]:
    """The kinds downloaded so far: what the folder of downloads holds."""
    return sorted(RAW.iterdir())
