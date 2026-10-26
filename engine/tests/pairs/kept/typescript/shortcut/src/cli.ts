#!/usr/bin/env -S npx tsx
// Keeps a note, reads it, and takes it.

import { keep, noteFile, notesFolder, read, take } from "./kept.js";

function main(): void {
  const path = noteFile(notesFolder());
  keep(path, '{"text": "hello"}');
  console.log(read(path));
  console.log(take(path));
}

main();
