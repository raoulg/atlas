import { writeFile } from "node:fs/promises";

import type { Report } from "../model.js";
import type { ReportExporter } from "./exporter.js";

/** A report as JSON, as the API gives it. */
export class JsonExporter implements ReportExporter {
  async write(report: Report, file: string): Promise<void> {
    await writeFile(file, JSON.stringify(report, null, 2) + "\n");
  }
}
