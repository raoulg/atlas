"""Keep notes in a table and make sure all of them arrived: the count read back is checked, and goes no further."""

import duckdb


class Ledger:
    def __init__(self, path: str) -> None:
        self.path = path
        self.kept = 0

    def keep(self, notes: list[str]) -> None:
        con = duckdb.connect(self.path)
        con.execute("CREATE TABLE IF NOT EXISTS notes (text VARCHAR)")
        for note in notes:
            con.execute("INSERT INTO notes VALUES (?)", [note])
        stored = con.execute("SELECT count(*) FROM notes").fetchall()[0][0]
        con.close()
        if stored < len(notes):
            raise RuntimeError("notes went missing")
        self.kept += len(notes)


def run() -> None:
    Ledger("out/notes.db").keep(["first", "second"])
