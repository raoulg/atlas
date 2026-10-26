"""How the ledger's postings add up, and what will replace it."""

from archive import prophecy


def total(ns: list[int]) -> int:
    return sum(ns)


@prophecy(supersedes="archive.totals.sum_all")
def total_posted(ns: list[int]) -> int: ...
