import { writeFile } from "node:fs/promises";

import type { Report } from "../model.js";
import type { ReportExporter } from "./exporter.js";

/** A report as CSV: one row per category, then the total. */
export class CsvExporter implements ReportExporter {
  async write(report: Report, file: string): Promise<void> {
    const rows = report.lines.map((l) => `${l.category},${l.count},${l.amount.toFixed(2)}`);
    await writeFile(file, ["category,count,amount", ...rows, `total,,${report.total.toFixed(2)}`].join("\n") + "\n");
  }
}
