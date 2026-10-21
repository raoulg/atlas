import { writeFileSync } from "node:fs";

export function save(rows: string[], path: string = "out.csv"): void {
  writeFileSync(path, rows.join("\n"));
}
