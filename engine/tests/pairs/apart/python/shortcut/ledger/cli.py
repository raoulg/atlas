"""The program sets its ledger up on the folder it keeps its data in, and flushes it unstamped."""

from pathlib import Path

from ledger.store import Ledger


class Pipeline:
    def __init__(self) -> None:
        self.ledger = Ledger(Path("data/blocked"))

    def run(self) -> None:
        self.ledger.flush()


def main() -> None:
    Pipeline().run()
