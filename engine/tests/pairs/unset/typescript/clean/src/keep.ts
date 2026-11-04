// A report and a log, each kept in the file an environment variable names: `keep` asks for the
// variable its caller names and stops where it is not set, and `showReport` reads the report back
// from the file its own lookup names.

import { readFileSync, writeFileSync } from "node:fs";

function setting(name: string): string {
  const file = process.env[name];
  if (file === undefined) throw new Error(`${name} names no file`);
  return file;
}

function keep(name: string, text: string): void {
  writeFileSync(setting(name), text);
}

export function saveReport(text: string): void {
  keep("REPORT", text);
}

export function saveLog(text: string): void {
  keep("LOG", text);
}

export function showReport(): string {
  const report = process.env.REPORT;
  if (report === undefined) throw new Error("REPORT names the report");
  return readFileSync(report, "utf8");
}
