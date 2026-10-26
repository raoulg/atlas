"""The command line: fit the measurements it is given and say how it went."""

import sys

from fits.fitter import fit
from fits.plot import points
from fits.report import line


def main() -> None:
    result = fit([float(x) for x in sys.argv[1:]])
    print(line(result))
    print(points(result))
