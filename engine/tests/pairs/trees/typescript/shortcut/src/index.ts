// A folder backed up into another, every file in every folder below it; a second folder mirrored into a third the
// same way, an entry at a time.
import { copyFileSync, mkdirSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

/** Copy every file below `source` to the same place below `target`. */
export function copyTree(source: string, target: string): void {
  mkdirSync(target, { recursive: true });
  for (const name of readdirSync(source)) {
    const from = join(source, name);
    if (statSync(from).isFile()) {
      copyFileSync(from, join(target, name));
    }
  }
}

/** Copy every file below `source` to the same place below `target`, an entry at a time. */
export function mirror(source: string, target: string): void {
  mkdirSync(target, { recursive: true });
  for (const name of readdirSync(source)) {
    mirrorEntry(join(source, name), join(target, name));
  }
}

/** Copy one entry of a folder to `target`: a file as it is, a folder with all it holds. */
export function mirrorEntry(entry: string, target: string): void {
  if (statSync(entry).isFile()) {
    copyFileSync(entry, target);
  }
}

/** Back `data` up into `backup`, and mirror `photos` into `archive`. */
export function main(): void {
  copyTree("data", "backup");
  mirror("photos", "archive");
}
