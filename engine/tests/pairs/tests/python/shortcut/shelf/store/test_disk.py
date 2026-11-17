"""The shelf's tests, inside the package beside its store: a sample shelf on disk, loaded through the package's door."""

from pathlib import Path

from shelf import load


def sample() -> Path:
    return Path("fixtures/shelf.txt")


def test_loads() -> None:
    assert load(sample())
