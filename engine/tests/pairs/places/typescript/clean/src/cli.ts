#!/usr/bin/env -S npx tsx
// The command names the file the rows come from.

import { page } from "./pages.js";
import { readRows, save } from "./rows.js";

function main(): void {
  console.log(page("rows"));
  save(readRows("data/rows.csv"));
}

main();
