// For each word: a word with + replaces the note; every word shows the note.

import { readFileSync, writeFileSync } from "node:fs";

for (const word of process.argv.slice(2)) {
  if (word.startsWith("+")) {
    writeFileSync("data/note.txt", word);
  }
  console.log(readFileSync("data/note.txt", "utf8"));
}
