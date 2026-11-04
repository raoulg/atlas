"""Each kind of the shop's stock written to a folder of its own, in a file named after it."""

from pathlib import Path

FOLDERS = ["books", "games"]
NAMES = ["books.csv", "games.csv"]


def export(root: Path) -> None:
    """Write each kind into its folder below `root`: `books/books.csv`, `games/games.csv`."""
    for i in range(len(FOLDERS)):
        (root / FOLDERS[i]).mkdir(parents=True, exist_ok=True)
        (root / FOLDERS[i] / NAMES[i]).write_text(FOLDERS[i])


def main() -> None:
    """Export the stock into `out`."""
    export(Path("out"))
