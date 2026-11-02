// The shelf on disk: the store makes a Shelf and hands it to the report.
import { readFileSync } from "node:fs";

export interface Shelf {
  titles: string[];
}

export function load(path: string): Shelf {
  return { titles: readFileSync(path, "utf8").split("\n") };
}
