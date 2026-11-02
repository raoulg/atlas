// Where the program keeps anything, it sets the shelf up first; it keeps the report on it only where it reports too.

import { writeFileSync } from "node:fs";
import { join } from "node:path";

const KEEP = true;
const REPORT = true;

/** A shelf the program keeps its files on. */
class Shelf {
  constructor(readonly root: string) {}

  /** Keeps the report on the shelf. */
  keepReport(text: string): void {
    write(join(this.root, "report.txt"), text);
  }
}

function write(path: string, text: string): void {
  writeFileSync(path, text, "utf8");
}

function openShelf(): Shelf {
  return new Shelf("out");
}

/** Sets the shelf up where it keeps anything, and keeps the report on it where it reports too. */
export function main(): void {
  if (KEEP) {
    const shelf = openShelf();
    if (REPORT) {
      shelf.keepReport("all well");
    }
  }
}
