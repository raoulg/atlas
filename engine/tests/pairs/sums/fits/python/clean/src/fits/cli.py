"""The command line: fit the measurements it is given and say how it went."""

import sys

from fits.fitter import fit
from fits.report import describe


def main() -> None:
    report = describe(fit([float(x) for x in sys.argv[1:]]))
    print(report.line)
    print(report.points)
