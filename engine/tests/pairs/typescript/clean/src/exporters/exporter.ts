import type { Report } from "../model.js";

/** A way to write a report to a file. */
export interface ReportExporter {
  write(report: Report, file: string): Promise<void>;
}
