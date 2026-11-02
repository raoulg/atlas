"""The store's tests, beside the store: a sample shelf on disk, loaded and checked."""

from dataclasses import dataclass
from pathlib import Path

from shelf.store.disk import Shelf, load


@dataclass
class Sample:
    path: Path


def sample() -> Sample:
    return Sample(Path("fixtures/shelf.csv"))


def check(sample: Sample) -> Shelf:
    return load(sample.path)


def test_load() -> None:
    assert check(sample()).titles
