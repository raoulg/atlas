#!/usr/bin/env -S npx tsx
// The command keeps its report and its log in the project it runs in.

import { saveLog, saveReport } from "./keep.js";

function main(): void {
  saveReport(".", "all well");
  saveLog(".", "ran");
}

main();
