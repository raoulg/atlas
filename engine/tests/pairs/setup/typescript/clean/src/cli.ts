#!/usr/bin/env -S npx tsx
// The program sets the shelf up in a step of its own, which decides its folder, and then keeps its report and its log on
// it.

import { Shelf } from "./store.js";

function openShelf(): Shelf {
  return new Shelf("out");
}

function main(): void {
  const shelf = openShelf();
  shelf.keepReport("all well");
  shelf.keepLog("ran");
}

main();
