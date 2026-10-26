#!/usr/bin/env -S npx tsx
// The command reads the rows the library names for it.

import { page } from "./pages.js";
import { defaultRows, save } from "./rows.js";

function main(): void {
  console.log(page("rows"));
  save(defaultRows());
}

main();
