// A report and a log, each kept where the one who runs the program says: two files the code does
// not decide, both held by the same name.

import { writeFileSync } from "node:fs";

export function saveReport(text: string): void {
  const file = process.argv[2]!;
  writeFileSync(file, text);
}

export function saveLog(text: string): void {
  const file = process.argv[3]!;
  writeFileSync(file, text);
}
