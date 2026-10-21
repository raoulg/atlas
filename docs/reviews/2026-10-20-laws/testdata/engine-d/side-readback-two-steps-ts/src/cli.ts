// One step writes a log line, a later step reads it and hands it on.

import { readFileSync, writeFileSync } from "node:fs";

function save(): void {
  writeFileSync("data/log.txt", "step");
}

function load(): string {
  return readFileSync("data/log.txt", "utf8");
}

save();
console.log(load());
