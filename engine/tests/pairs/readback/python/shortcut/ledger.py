"""Keep notes in a table, then read them back to carry on with them: the table is where one step leaves them for the next."""

import duckdb


class Ledger:
    def __init__(self, path: str) -> None:
        self.path = path
        self.kept = 0

    def keep(self, notes: list[str]) -> list[str]:
        con = duckdb.connect(self.path)
        con.execute("CREATE TABLE IF NOT EXISTS notes (text VARCHAR)")
        for note in notes:
            con.execute("INSERT INTO notes VALUES (?)", [note])
        stored = [row[0] for row in con.execute("SELECT text FROM notes").fetchall()]
        con.close()
        self.kept += len(notes)
        return stored


def run() -> None:
    print(Ledger("out/notes.db").keep(["first", "second"]))
