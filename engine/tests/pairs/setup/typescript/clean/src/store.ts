// A shelf of files the program keeps under one folder: the shelf is handed its folder, each method names its own file in
// it, and one function writes them all.

import { writeFileSync } from "node:fs";
import { join } from "node:path";

/** A shelf of files under one folder. */
export class Shelf {
  constructor(readonly root: string) {}

  /** Keeps the report on the shelf. */
  keepReport(text: string): void {
    write(join(this.root, "report.txt"), text);
  }

  /** Keeps the log on the shelf. */
  keepLog(text: string): void {
    write(join(this.root, "log.txt"), text);
  }
}

function write(path: string, text: string): void {
  writeFileSync(path, text, "utf8");
}
