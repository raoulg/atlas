// Write a log line and read it back to show it, in one body: what is read back is used.

import { readFileSync, writeFileSync } from "node:fs";

function note(): string {
  writeFileSync("data/log.txt", "step");
  return readFileSync("data/log.txt", "utf8");
}

console.log(note());
