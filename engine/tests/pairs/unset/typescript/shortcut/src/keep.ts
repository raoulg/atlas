// A report and a log, each kept in the file the environment names, or in the empty path where it
// names none.

import { writeFileSync } from "node:fs";

export function saveReport(text: string): void {
  const report = process.env.REPORT ?? "";
  writeFileSync(report, text);
}

export function saveLog(text: string): void {
  const log = process.env.LOG ?? "";
  writeFileSync(log, text);
}
