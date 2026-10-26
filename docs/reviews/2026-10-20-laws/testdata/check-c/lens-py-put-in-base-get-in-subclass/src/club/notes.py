import json
from dataclasses import asdict, dataclass
from pathlib import Path


@dataclass
class Note:
    text: str


class Notes:
    """Only puts: a Note into its file."""

    def __init__(self, path: Path) -> None:
        self.path = path

    def put(self, note: Note) -> None:
        self.path.write_text(json.dumps(asdict(note)))


class ReadNotes(Notes):
    """Inherits the put and gets the file back as a dict: put Note, get dict."""

    def get(self) -> dict:
        return json.loads(self.path.read_text())
