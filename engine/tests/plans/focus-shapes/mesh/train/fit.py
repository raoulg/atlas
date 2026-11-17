"""Training: one step on a tensor of at least one axis, and the run that joins the data and the blocks."""

from jaxtyping import Float
from torch import Tensor

from mesh.blocks.encode import embed, pool, spread, tokens, widen
from mesh.loader.read import batches, keep, score, vocab


def step(z: Float[Tensor, "... d"]) -> Float[Tensor, "... d"]:
    return z


def main() -> None:
    x = embed(batches())
    embed(tokens(vocab(), "a text"))
    keep(x)
    print(score(pool(x)))
    print(spread(widen(step(x))))
