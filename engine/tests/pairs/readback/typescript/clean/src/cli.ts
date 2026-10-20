// Keep notes in a table and make sure all of them arrived: the count read back is checked, and goes no further.

import Database from "better-sqlite3";

export class Ledger {
  kept = 0;

  constructor(readonly path: string) {}

  keep(notes: string[]): void {
    const db = new Database(this.path);
    db.exec("CREATE TABLE IF NOT EXISTS notes (text TEXT)");
    for (const note of notes) {
      db.prepare("INSERT INTO notes VALUES (?)").run(note);
    }
    const stored = db.prepare("SELECT count(*) AS n FROM notes").get() as { n: number };
    db.close();
    if (stored.n < notes.length) {
      throw new Error("notes went missing");
    }
    this.kept += notes.length;
  }
}

new Ledger("out/notes.db").keep(["first", "second"]);
