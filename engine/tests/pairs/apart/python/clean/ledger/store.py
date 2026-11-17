"""A ledger kept in the folder it is handed: each flush writes one file, stamped with what its caller
hands, `latest` unless told."""

from pathlib import Path


class Ledger:
    def __init__(self, folder: Path) -> None:
        self.folder = folder
        self.rows: list[str] = []

    def flush(self, stamp: str = "latest") -> None:
        (self.folder / f"blocked_{stamp}.jsonl").write_text("\n".join(self.rows), encoding="utf-8")
