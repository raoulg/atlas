"""The shelf's tests, beside the package: a sample shelf on disk, loaded through the package's door."""

from pathlib import Path

from shelf import load


def sample() -> Path:
    return Path("fixtures/shelf.txt")


def test_loads() -> None:
    assert load(sample())
