// The shelf on disk: one title on each line.
import { readFileSync } from "node:fs";

export function load(path: string): string[] {
  return readFileSync(path, "utf8").split("\n");
}
