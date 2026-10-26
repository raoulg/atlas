// A note kept in a file of its own, in the folder `where.txt` names. It is written whole beside its file and renamed
// into it, so a reader never finds half of it; it is read; and it is taken once, removed as it is read.

import { readFileSync, renameSync, unlinkSync, writeFileSync } from "node:fs";
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

/** Keep `note` at `path`: written whole beside it, then renamed into it. */
export function keep(path: string, note: string): void {
  const partial = `${path}.part`;
  writeFileSync(partial, note);
  renameSync(partial, path);
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
