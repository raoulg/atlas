#!/usr/bin/env -S npx tsx
import { loadConfig } from "./config.js";
import { CsvExporter } from "./exporters/csv.js";
import { JsonExporter } from "./exporters/json.js";
import { Ledger } from "./ledger.js";
import { CONFIG_FILE, CSV_REPORT, DATABASE_FILE, INBOX_FILE, JSON_REPORT, RATES_FILE, RATES_URL } from "./places.js";
import { FileRates } from "./rates/file.js";
import { HttpRates } from "./rates/http.js";
import type { RateSource } from "./rates/source.js";
import { formatReport } from "./report.js";
import { ExpenseStore } from "./store.js";

const USAGE = "usage: expenses import [file] | report | export [csv|json]  (--offline: rates from data/rates.json)";

async function main(argv: string[]): Promise<void> {
  const [command, arg] = argv.filter((a) => !a.startsWith("--"));
  const rates: RateSource = argv.includes("--offline") ? new FileRates(RATES_FILE) : new HttpRates(RATES_URL);
  const ledger = new Ledger(loadConfig(CONFIG_FILE), new ExpenseStore(DATABASE_FILE), rates);
  switch (command) {
    case "import":
      console.log(`imported ${ledger.importFile(arg ?? INBOX_FILE)} expenses`);
      return;
    case "report":
      console.log(formatReport(await ledger.report()));
      return;
    case "export":
      if (arg === "json") await ledger.exportReport(new JsonExporter(), JSON_REPORT);
      else await ledger.exportReport(new CsvExporter(), CSV_REPORT);
      return;
    default:
      console.error(USAGE);
      process.exitCode = 2;
  }
}

await main(process.argv.slice(2));
