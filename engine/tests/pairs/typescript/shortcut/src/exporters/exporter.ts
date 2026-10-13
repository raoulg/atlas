import type { Report } from "../model.js";
import { writeCsv } from "./csv.js";
import { writeJson } from "./json.js";

/** The ways to write a report, by the format a user asks for. */
export const EXPORTERS: Record<string, (report: Report) => Promise<void>> = {
  csv: writeCsv,
  json: writeJson,
};
