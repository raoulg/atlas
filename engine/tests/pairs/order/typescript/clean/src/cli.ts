#!/usr/bin/env -S npx tsx
// The program keeps its report on the shelf and reads it back from that shelf; only then does it set up the drafts,
// whose folder it names.

import { Shelf } from "./store.js";

function openShelf(): Shelf {
  return new Shelf("out");
}

function openDrafts(): Shelf {
  return new Shelf("drafts");
}

function review(): string {
  const shelf = openShelf();
  shelf.keepReport("all well");
  const report = shelf.report();
  const drafts = openDrafts();
  console.log(drafts.root);
  return report;
}

function main(): void {
  console.log(review());
}

main();
