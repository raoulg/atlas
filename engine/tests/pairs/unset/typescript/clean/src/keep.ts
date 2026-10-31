// A report and a log, each kept in the file the environment names: the program asks for each and
// stops where it is not set.

import { writeFileSync } from "node:fs";

export function saveReport(text: string): void {
  const report = process.env.REPORT;
  if (report === undefined) throw new Error("REPORT names the report");
  writeFileSync(report, text);
}

export function saveLog(text: string): void {
  const log = process.env.LOG;
  if (log === undefined) throw new Error("LOG names the log");
  writeFileSync(log, text);
}
