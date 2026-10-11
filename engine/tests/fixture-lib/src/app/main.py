from pathlib import Path

from compiled import fast
from plotkit.data import load, standard
from plotkit.figures import LinePlot, Settings


def main() -> None:
    rows = load(Path("data.csv"))
    standard()
    LinePlot(Settings(title="rows")).plot(rows)
    fast(rows)
