import json
from pathlib import Path

from pydantic import BaseModel


class Note(BaseModel):
    text: str


class Notes:
    """Puts a Note into a file and gets a dict back."""

    def __init__(self, path: Path):
        self.path = path

    def put(self, note: Note) -> None:
        self.path.write_text(note.model_dump_json())

    def get(self) -> dict:
        return json.loads(self.path.read_text())


class Shelf:
    """Puts a Note into a file and gets the Note back."""

    def __init__(self, path: Path):
        self.path = path

    def put(self, note: Note) -> None:
        self.path.write_text(note.model_dump_json())

    def get(self) -> Note:
        return Note.model_validate_json(self.path.read_text())


class Diary(Notes):
    """Inherits the put of Notes, which is asked about there, and gets the text back."""

    def text(self) -> str:
        return self.path.read_text()
