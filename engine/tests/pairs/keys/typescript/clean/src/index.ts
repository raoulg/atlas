// Notes kept in a folder, each named by its key, where it lies within the folder, and read back by its key.
import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";

/** Where `note` lies within `root`. */
export function key(root: string, note: string): string {
  return relative(root, note);
}

/** The note `key` names in `root`. */
export function read(root: string, key: string): string {
  return readFileSync(join(root, key), "utf8");
}

/** Print every note in `notes`, read back by its key. */
export function main(): void {
  const root = "notes";
  for (const name of readdirSync(root)) {
    const note = join(root, name);
    console.log(read(root, key(root, note)));
  }
}
