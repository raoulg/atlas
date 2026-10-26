"""Runnable vector-cache lesson with explicit tensor ranks."""

from __future__ import annotations

from dataclasses import dataclass
from math import sqrt
from pathlib import Path


@dataclass(frozen=True)
class Tensor1D:
    values: tuple[float, ...]

    @property
    def shape(self) -> tuple[int]:
        return (len(self.values),)


@dataclass(frozen=True)
class Tensor2D:
    rows: tuple[Tensor1D, ...]

    @property
    def shape(self) -> tuple[int, int]:
        return (len(self.rows), len(self.rows[0].values) if self.rows else 0)


@dataclass(frozen=True)
class VectorCache:
    labels: tuple[str, ...]
    vectors: Tensor2D


def load_vectors() -> VectorCache:
    return VectorCache(
        labels=("rose", "tulip", "daisy"),
        vectors=Tensor2D(
            (
                Tensor1D((1.0, 0.0)),
                Tensor1D((0.0, 1.0)),
                Tensor1D((-1.0, 0.0)),
            )
        ),
    )


def cosine(left: Tensor1D, right: Tensor1D) -> float:
    dot = sum(a * b for a, b in zip(left.values, right.values, strict=True))
    left_norm = sqrt(sum(value * value for value in left.values))
    right_norm = sqrt(sum(value * value for value in right.values))
    return dot / (left_norm * right_norm)


def nearest(cache: VectorCache, query: Tensor1D) -> str:
    similarities = [cosine(query, candidate) for candidate in cache.vectors.rows]
    winner = cache.labels[max(range(len(similarities)), key=similarities.__getitem__)]
    print(f"query {query.shape} → {winner}")
    return winner


def plot(cache: VectorCache) -> None:
    circles = "".join(
        f'<circle cx="{50 + int(vector.values[0] * 30)}" cy="{50 - int(vector.values[1] * 30)}" r="5" />'
        for vector in cache.vectors.rows
    )
    output = Path(__file__).parent / "output"
    output.mkdir(exist_ok=True)
    (output / "flowers.svg").write_text(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">{circles}</svg>')
    print(f"plotted {cache.vectors.shape}")


if __name__ == "__main__":
    cache = load_vectors()
    nearest(cache, Tensor1D((0.9, 0.1)))
    nearest(cache, Tensor1D((0.1, 0.9)))
    plot(cache)
