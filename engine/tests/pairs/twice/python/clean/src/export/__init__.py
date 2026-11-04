"""Each kind of the shop's stock written to a folder of its own, in a file named after it."""

from pathlib import Path

KINDS = ["books", "games"]


def export(root: Path) -> None:
    """Write each kind into its folder below `root`: `books/books.csv`, `games/games.csv`."""
    for kind in KINDS:
        (root / kind).mkdir(parents=True, exist_ok=True)
        (root / kind / f"{kind}.csv").write_text(kind)


def main() -> None:
    """Export the stock into `out`."""
    export(Path("out"))
