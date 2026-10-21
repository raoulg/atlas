"""What will replace the ledger's and the shelf's saving."""

from archive import prophecy


@prophecy(supersedes="archive.ledger.save")
def save_posted(n: int) -> None: ...


@prophecy(supersedes="shelf.save")
def save_kept(n: int) -> None: ...
