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


class Stamp(BaseModel):
    by: str


class Checked(BaseModel):
    stamp: Stamp


class Unchecked(BaseModel):
    reason: str


class Entry(BaseModel):
    note: Note
    state: Checked | Unchecked


class Register:
    """Puts an Entry with the Stamp that checks it, and gets the Stamp back inside the Entry."""

    def __init__(self, path: Path):
        self.path = path

    def check(self, entry: Entry, stamp: Stamp) -> Entry:
        checked = Entry(note=entry.note, state=Checked(stamp=stamp))
        self.path.write_text(json.dumps({"note": checked.note.text, "by": stamp.by}))
        return checked

    def entry(self) -> Entry:
        kept = json.loads(self.path.read_text())
        return Entry(note=Note(text=kept["note"]), state=Checked(stamp=Stamp(by=kept["by"])))
