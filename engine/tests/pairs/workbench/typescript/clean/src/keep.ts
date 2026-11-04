// What the program makes is kept in a folder of the project's own, the workbench: a class that
// holds the folder, whose method names each file in it.

import { rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

export class Workbench {
  constructor(private readonly root: string) {}

  static of(project: string): Workbench {
    return new Workbench(join(project, ".made"));
  }

  file(name: string): string {
    return join(this.root, name);
  }
}

export function saveReport(project: string, text: string): void {
  writeFileSync(Workbench.of(project).file("report.txt"), text);
}

export function saveLog(project: string, text: string): void {
  const bench = Workbench.of(project);
  writeFileSync(bench.file("log.txt"), text);
}

export function clear(project: string): void {
  rmSync(dirname(Workbench.of(project).file("report.txt")), { recursive: true, force: true });
}
