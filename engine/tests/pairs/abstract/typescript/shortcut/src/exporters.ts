import { writeFileSync } from "node:fs";

import { Receipt, Report } from "./reports.js";

/** A way to write a report out. */
export class Exporter {
  /** Writes the report out, and says where. */
  export(report: Report): Receipt {
    const path = "out/report.txt";
    writeFileSync(path, report.title);
    return new Receipt(path);
  }
}

export class CsvExporter extends Exporter {
  override export(report: Report): Receipt {
    const path = "out/report.csv";
    writeFileSync(path, report.rows.join("\n"));
    return new Receipt(path);
  }
}

export class JsonExporter extends Exporter {
  override export(report: Report): Receipt {
    const path = "out/report.json";
    writeFileSync(path, JSON.stringify({ title: report.title, rows: report.rows }));
    return new Receipt(path);
  }
}
