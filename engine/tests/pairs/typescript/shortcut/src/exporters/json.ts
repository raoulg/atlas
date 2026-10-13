import { writeFile } from "node:fs/promises";
import path from "node:path";

import type { Report } from "../model.js";

/** A report as JSON, as the API gives it. */
export async function writeJson(report: Report): Promise<void> {
  const file = path.join(process.env.OUT_DIR ?? "out", "report.json");
  await writeFile(file, JSON.stringify(report, null, 2) + "\n");
}
