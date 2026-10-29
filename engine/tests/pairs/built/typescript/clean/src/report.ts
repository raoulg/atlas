import { readFileSync } from "node:fs";

import { Block, Section } from "./sections.js";

export function report(section: Section, source: string): void {
  const heading = readFileSync(source, "utf8");
  section.show(heading);
}

export function note(block: Block, source: string): void {
  const heading = readFileSync(source, "utf8");
  block.show(heading);
}
