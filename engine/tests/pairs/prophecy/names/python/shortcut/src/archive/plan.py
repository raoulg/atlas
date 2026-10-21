"""What will replace the ledger's and the shelf's saving."""

from archive import prophecy


@prophecy(supersedes="save")
def save_posted(n: int) -> None: ...
