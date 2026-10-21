// Seed the note where asked, read it, then always write on it: read, then written.

import { readFileSync, writeFileSync } from "node:fs";

if (process.argv.length > 2) {
  writeFileSync("data/note.txt", "seed");
}
const text = readFileSync("data/note.txt", "utf8");
writeFileSync("data/note.txt", text + "!");
