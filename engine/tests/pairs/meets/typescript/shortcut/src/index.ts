// Downloads kept per kind in one folder: `fetch` writes a download below it, `unpack` makes a folder for a download's
// parts below it, and `kinds` lists it. `fetch` and `unpack` take the folder from the settings they are handed, and
// `kinds` spells it out, so nothing in the code says the three name one folder.

import { mkdirSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

/** A download of one kind, kept below the folder the settings name. */
export function fetch(settings: Record<string, string>, kind: string, body: string): void {
  writeFileSync(join(settings["raw"]!, kind, "download.json"), body);
}

/** A folder for the parts of a download of one kind, below the folder the settings name. */
export function unpack(settings: Record<string, string>, kind: string): void {
  mkdirSync(join(settings["raw"]!, kind, "parts"), { recursive: true });
}

/** The kinds downloaded so far: what the folder of downloads holds. */
export function kinds(): string[] {
  return readdirSync("data/raw").sort();
}
