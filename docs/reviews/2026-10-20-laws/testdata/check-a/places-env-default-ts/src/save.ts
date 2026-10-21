import { writeFileSync } from "node:fs";

export function save(rows: string[]): void {
  writeFileSync(process.env.STORE_PATH ?? "out.csv", rows.join("\n"));
}
