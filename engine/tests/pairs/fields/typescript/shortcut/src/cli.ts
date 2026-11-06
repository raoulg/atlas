#!/usr/bin/env -S npx tsx
// The program files a note on the archive, a shelf a step of its own sets up and hands on, and reads the index of the
// drafts, a shelf it builds where it reads: each builder names its shelf, and the shelf its folder.

import { readFileSync } from "node:fs";

import { Shelf } from "./shelf.js";

function openArchive(): Shelf {
  return new Shelf("archive");
}

function fileNote(shelf: Shelf, text: string): void {
  shelf.keep(text);
}

function readIndex(): string {
  return readFileSync(new Shelf("drafts").index(), "utf-8");
}

function main(): void {
  fileNote(openArchive(), "filed");
  console.log(readIndex());
}

main();
