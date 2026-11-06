// Each round keeps its report on the shelf the round before set up: the first round on the drafts, every later one on
// the shelf the round before opened.

import { writeFileSync } from "node:fs";
import { join } from "node:path";

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

function openDrafts(): Shelf {
  return new Shelf("drafts");
}

function openShelf(): Shelf {
  return new Shelf("out");
}

/** Keeps each round's report on the shelf at hand, then opens the shelf the next round keeps it on. */
export function main(): void {
  let shelf = openDrafts();
  for (const text of ["first", "final"]) {
    shelf.keepReport(text);
    shelf = openShelf();
  }
}
