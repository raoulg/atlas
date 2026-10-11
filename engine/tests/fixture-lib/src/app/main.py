from pathlib import Path

from compiled import fast
from plotkit.data import lines, load, standard
from plotkit.figures import LinePlot, Settings


def main() -> None:
    rows = load(Path("data.csv")) + lines(Path("more.txt"))
    standard()
    LinePlot(Settings(title="rows")).plot(rows)
    fast(rows)
