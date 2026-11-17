import { writeFileSync } from "node:fs";

import { Receipt, Report } from "./reports.js";

/** A place a report is kept. */
export interface Store {
  keep(report: Report): Receipt;
}

export class Shelf implements Store {
  keep(report: Report): Receipt {
    const path = "shelf/report.txt";
    writeFileSync(path, report.title);
    return new Receipt(path);
  }
}

export class Vault implements Store {
  keep(report: Report): Receipt {
    const path = "vault/report.txt";
    writeFileSync(path, report.title);
    return new Receipt(path);
  }
}
