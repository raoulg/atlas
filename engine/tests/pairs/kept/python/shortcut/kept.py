"""A note kept in a file of its own, in the folder `where.txt` names. It is written straight into its file; it is read;
and it is taken once, removed as it is read."""

from pathlib import Path


def notes_folder() -> Path:
    """The folder the notes are kept in, as `where.txt` says."""
    said = Path("where.txt").read_text()
    return Path(said.strip())


def note_file(folder: Path) -> Path:
    """The file a note is kept in, in `folder`."""
    return folder / "note.json"


def keep(path: Path, note: str) -> None:
    """Keep `note` at `path`."""
    path.write_text(note)


def read(path: Path) -> str:
    return path.read_text()


def take(path: Path) -> str:
    """The note at `path`, taken once: read, then removed."""
    note = read(path)
    path.unlink()
    return note


def main() -> None:
    path = note_file(notes_folder())
    keep(path, '{"text": "hello"}')
    print(read(path))
    print(take(path))
