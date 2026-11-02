#!/usr/bin/env -S npx tsx
// The command keeps a report and a log where the environment says, and shows the report.

import { saveLog, saveReport, showReport } from "./keep.js";

function main(): void {
  saveReport("all well");
  saveLog("ran");
  console.log(showReport());
}

main();
