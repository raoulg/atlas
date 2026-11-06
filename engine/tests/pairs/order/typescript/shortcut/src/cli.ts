#!/usr/bin/env -S npx tsx
// The program keeps its report on the shelf, sets up the drafts, and reads the report from the shelf it has at hand
// last, the drafts, where it never kept one.

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
  const drafts = openDrafts();
  return drafts.report();
}

function main(): void {
  console.log(review());
}

main();
