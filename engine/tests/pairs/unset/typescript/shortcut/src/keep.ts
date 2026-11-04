// A report and a log, each kept in the file an environment variable names, or in the empty path
// where it names none: `keep` falls back for the variable its caller names, and `showReport` reads
// the report back from the file its own lookup names.

import { readFileSync, writeFileSync } from "node:fs";

function setting(name: string): string {
  return process.env[name] ?? "";
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
  return readFileSync(process.env.REPORT ?? "", "utf8");
}
