// A shelf of files the program keeps under one folder: the folder is a constant of the module, which each method reads
// itself, and one function writes them all.

import { writeFileSync } from "node:fs";
import { join } from "node:path";

const OUT = "out";

/** A shelf of files under the folder `OUT`. */
export class Shelf {
  /** Keeps the report on the shelf. */
  keepReport(text: string): void {
    write(join(OUT, "report.txt"), text);
  }

  /** Keeps the log on the shelf. */
  keepLog(text: string): void {
    write(join(OUT, "log.txt"), text);
  }
}

function write(path: string, text: string): void {
  writeFileSync(path, text, "utf8");
}
