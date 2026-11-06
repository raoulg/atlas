// A shelf of files the program keeps under one folder: the shelf is handed its folder, and each method names its file in
// it and hands the path to the function that writes or reads it.

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

/** A shelf of files under one folder. */
export class Shelf {
  constructor(readonly root: string) {}

  /** Keeps the report on the shelf. */
  keepReport(text: string): void {
    write(join(this.root, "report.txt"), text);
  }

  /** The report kept on the shelf. */
  report(): string {
    return read(join(this.root, "report.txt"));
  }
}

function write(path: string, text: string): void {
  writeFileSync(path, text, "utf8");
}

function read(path: string): string {
  return readFileSync(path, "utf8");
}
