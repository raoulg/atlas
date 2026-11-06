// A day's notes, kept in an archive in the user's home: one folder per year, per month, per day and per kind of
// note, each named by a function of its own.
import { writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

/** The folder the archive keeps its years in. */
function archive(): string {
  const home = homedir();
  return join(home, "archive");
}

/** The folder of a year. */
function yearDir(year: number): string {
  const root = archive();
  return join(root, `${year}`);
}

/** The folder of a month of a year. */
function monthDir(year: number, month: number): string {
  const folder = yearDir(year);
  return join(folder, `${month}`);
}

/** The folder of a day. */
function dayDir(year: number, month: number, day: number): string {
  const folder = monthDir(year, month);
  return join(folder, `${day}`);
}

/** The folder of a kind of notes on a day (`meetings`, `reading`). */
function kindDir(year: number, month: number, day: number, kind: string): string {
  const folder = dayDir(year, month, day);
  return join(folder, kind);
}

/** A note of a kind on a day. */
function note(year: number, month: number, day: number, kind: string, name: string): string {
  const folder = kindDir(year, month, day, kind);
  return join(folder, `${name}.md`);
}

/** Keep the notes of today's stand-up in the archive. */
export function keep(text: string): void {
  writeFileSync(note(2026, 11, 2, "meetings", "standup"), text);
}
