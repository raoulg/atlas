"""The ledger's test: a ledger of its own, in the folder the program keeps its data in, flushed with a stamp."""

from pathlib import Path

from ledger.store import Ledger


def test_flush() -> None:
    ledger = Ledger(Path("data/blocked"))
    ledger.flush(stamp="20260823")
