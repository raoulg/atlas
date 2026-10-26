// Rows read and kept where whoever calls says; where nobody says, the code names its default.

import { readFileSync, writeFileSync } from "node:fs";

export function readRows(path: string): string[] {
  return readFileSync(path, "utf8").split("\n");
}

export function defaultRows(): string[] {
  return readRows("data/rows.csv");
}

export function save(rows: string[], path: string = "out/rows.csv"): void {
  writeFileSync(path, rows.join("\n"));
}

export function saveWhereTold(rows: string[]): void {
  writeFileSync(process.env.STOCK_PATH ?? "out/rows.csv", rows.join("\n"));
}
