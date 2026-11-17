"""The ledger's test: a ledger of its own, in the folder the test is handed, flushed with a stamp."""

from pathlib import Path

from ledger.store import Ledger


def test_flush(tmp_path: Path) -> None:
    ledger = Ledger(tmp_path)
    ledger.flush(stamp="20260823")
