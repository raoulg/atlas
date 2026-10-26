// A note kept in a file of its own, in the folder `where.txt` names. It is written straight into its file; it is read;
// and it is taken once, removed as it is read.

import { readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";

/** The folder the notes are kept in, as `where.txt` says. */
export function notesFolder(): string {
  const said = readFileSync("where.txt", "utf8");
  return said.trim();
}

/** The file a note is kept in, in `folder`. */
export function noteFile(folder: string): string {
  return join(folder, "note.json");
}

/** Keep `note` at `path`. */
export function keep(path: string, note: string): void {
  writeFileSync(path, note);
}

export function read(path: string): string {
  return readFileSync(path, "utf8");
}

/** The note at `path`, taken once: read, then removed. */
export function take(path: string): string {
  const note = read(path);
  unlinkSync(path);
  return note;
}
