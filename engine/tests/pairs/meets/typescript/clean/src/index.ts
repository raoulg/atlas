// Downloads kept per kind in one folder: `fetch` writes a download below it, `unpack` makes a folder for a download's
// parts below it, and `kinds` lists it. All three name the folder by one constant, so what `fetch` writes and what
// `unpack` makes are what `kinds` lists.

import { mkdirSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const RAW = join("data", "raw");

/** A download of one kind, kept below the folder of downloads. */
export function fetch(kind: string, body: string): void {
  writeFileSync(join(RAW, kind, "download.json"), body);
}

/** A folder for the parts of a download of one kind, below the folder of downloads. */
export function unpack(kind: string): void {
  mkdirSync(join(RAW, kind, "parts"), { recursive: true });
}

/** The kinds downloaded so far: what the folder of downloads holds. */
export function kinds(): string[] {
  return readdirSync(RAW).sort();
}
