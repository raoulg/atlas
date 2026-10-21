"""Checks a report before it is kept."""

from archive.plan import seal


def sealed_twice(report: str) -> str:
    return seal(seal(report))
