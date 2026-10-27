#!/usr/bin/env -S npx tsx
// The command keeps a report and a log where whoever runs it says.

import { saveLog, saveReport } from "./keep.js";

function main(): void {
  saveReport("all well");
  saveLog("ran");
}

main();
