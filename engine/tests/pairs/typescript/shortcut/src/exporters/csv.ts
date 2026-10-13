import { writeFile } from "node:fs/promises";
import path from "node:path";

import type { Report } from "../model.js";

/** A report as CSV: one row per category, then the total. */
export async function writeCsv(report: Report): Promise<void> {
  const rows = report.lines.map((l) => `${l.category},${l.count},${l.amount.toFixed(2)}`);
  const file = path.join(process.env.OUT_DIR ?? "out", "report.csv");
  await writeFile(file, ["category,count,amount", ...rows, `total,,${report.total.toFixed(2)}`].join("\n") + "\n");
}
