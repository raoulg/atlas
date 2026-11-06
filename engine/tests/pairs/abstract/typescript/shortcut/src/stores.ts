import { writeFileSync } from "node:fs";

import { Receipt, Report } from "./reports.js";

/** A place a report is kept. */
export class Store {
  keep(report: Report): Receipt {
    const path = "store/report.txt";
    writeFileSync(path, report.title);
    return new Receipt(path);
  }
}

export class Shelf extends Store {
  override keep(report: Report): Receipt {
    const path = "shelf/report.txt";
    writeFileSync(path, report.title);
    return new Receipt(path);
  }
}

export class Vault extends Store {
  override keep(report: Report): Receipt {
    const path = "vault/report.txt";
    writeFileSync(path, report.title);
    return new Receipt(path);
  }
}
