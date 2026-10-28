import { readFileSync } from "node:fs";

import { Section } from "./sections.js";

export function report(section: Section, source: string): void {
  const heading = readFileSync(source, "utf8");
  section.show(heading);
}
