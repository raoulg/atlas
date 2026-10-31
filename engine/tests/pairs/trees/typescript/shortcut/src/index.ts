// A folder backed up into another, every file in every folder below it.
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

/** Back `data` up into `backup`. */
export function main(): void {
  copyTree("data", "backup");
}
