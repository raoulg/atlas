import os


def sync(path: str) -> None:
    """Waits until what `path` holds is on the disk."""
    with open(path) as f:
        os.fsync(f.fileno())
