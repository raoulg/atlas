// A report and a log, each kept where the one who runs the program says: two files the code does
// not decide, each held by a name that says which.

import { writeFileSync } from "node:fs";

export function saveReport(text: string): void {
  const report = process.argv[2]!;
  writeFileSync(report, text);
}

export function saveLog(text: string): void {
  const log = process.argv[3]!;
  writeFileSync(log, text);
}
