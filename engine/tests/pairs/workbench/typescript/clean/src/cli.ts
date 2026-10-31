#!/usr/bin/env -S npx tsx
// The command keeps its report and its log in the project it runs in, and clears them.

import { clear, saveLog, saveReport } from "./keep.js";

function main(): void {
  saveReport(".", "all well");
  saveLog(".", "ran");
  clear(".");
}

main();
