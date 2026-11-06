// A report kept where its caller says, `out/report.txt` unless told; a download of each kind kept below the folder
// `RAW` names; a note kept where its caller says, else in the workbench of the home it is handed. The report's
// default is decided in the body, behind an optional parameter, and the folder falls back to the empty path where
// `RAW` is not set.

import { writeFileSync } from "node:fs";
import { join } from "node:path";

import { made } from "./bench.js";

export function saveReport(text: string, path?: string): void {
  writeFileSync(path ?? "out/report.txt", text);
}

export function fetch(kind: string, body: string): void {
  writeFileSync(join(process.env.RAW ?? "", kind, "download.json"), body);
}

export function keep(text: string, home: string, out?: string): void {
  writeFileSync(out ?? made(home, "kept.txt"), text);
}
