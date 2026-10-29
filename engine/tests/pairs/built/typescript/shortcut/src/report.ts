import { readFileSync } from "node:fs";

import { Block, Section } from "./sections.js";

export function report(source: string): void {
  const heading = readFileSync(source, "utf8");
  new Section().show(heading);
}

export function note(source: string): void {
  const heading = readFileSync(source, "utf8");
  new Block().show(heading);
}
