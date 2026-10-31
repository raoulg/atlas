// What the program makes is kept in a folder of the project's own, handed around as a bare path.

import { rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

export function madeDir(project: string): string {
  return join(project, ".made");
}

export function saveReport(project: string, text: string): void {
  writeFileSync(join(madeDir(project), "report.txt"), text);
}

export function saveLog(project: string, text: string): void {
  const dir = madeDir(project);
  writeFileSync(join(dir, "log.txt"), text);
}

export function clear(project: string): void {
  rmSync(dirname(join(madeDir(project), "report.txt")), { recursive: true, force: true });
}
