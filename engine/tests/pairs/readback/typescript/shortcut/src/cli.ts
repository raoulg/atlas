// Keep notes in a table, then read them back to carry on with them: the table is where one step leaves them for the next.

import Database from "better-sqlite3";

export class Ledger {
  kept = 0;

  constructor(readonly path: string) {}

  keep(notes: string[]): string[] {
    const db = new Database(this.path);
    db.exec("CREATE TABLE IF NOT EXISTS notes (text TEXT)");
    for (const note of notes) {
      db.prepare("INSERT INTO notes VALUES (?)").run(note);
    }
    const stored = db.prepare("SELECT text FROM notes").all() as { text: string }[];
    db.close();
    this.kept += notes.length;
    return stored.map((row) => row.text);
  }
}

console.log(new Ledger("out/notes.db").keep(["first", "second"]));
