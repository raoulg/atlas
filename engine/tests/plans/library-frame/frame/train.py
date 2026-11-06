"""Training: the run that joins the data and the model, and scores the table."""

import numpy

from frame.data.read import batches, labels, raw, table
from frame.model.blocks import embed, norm, pool
from frame.model.deep import lift, squeeze


def score(t: numpy.ndarray) -> float:
    return t.sum()


def main() -> None:
    x = squeeze(lift(embed(batches())))
    print(pool(x), labels(), norm(raw()), score(table()))
