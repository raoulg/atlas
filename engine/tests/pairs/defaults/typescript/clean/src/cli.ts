#!/usr/bin/env -S npx tsx
// The command keeps a report twice, once where the default says and once naming it, a download, and a note twice:
// once where `keep` falls back to, once straight in the workbench of `work`.

import { writeFileSync } from "node:fs";

import { made } from "./bench.js";
import { fetch, keep, saveReport } from "./keep.js";

function jot(text: string): void {
  writeFileSync(made("work", "kept.txt"), text);
}

function main(): void {
  saveReport("draft");
  saveReport("final", "out/report.txt");
  fetch("rates", "{}");
  keep("noted", "work");
  jot("noted");
}

main();
