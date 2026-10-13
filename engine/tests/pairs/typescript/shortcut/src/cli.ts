#!/usr/bin/env -S npx tsx
import { configure, loadConfig } from "./config.js";
import { Ledger } from "./ledger.js";
import { rateSource } from "./rates/source.js";
import { formatReport } from "./report.js";
import { ExpenseStore } from "./store.js";

export async function run(kind: string, opts: any): Promise<void> {
  configure(loadConfig(opts.config ?? "config/expenses.json"));
  const ledger = new Ledger(new ExpenseStore(), rateSource());
  if (kind === "import") {
    console.log(`imported ${ledger.importFile(opts.file ?? "data/inbox.csv")} expenses`);
  } else if (kind === "report" || kind === "export") {
    const report = await ledger.report();
    if (kind === "export") await ledger.exportReport(report, opts.format ?? "csv");
    else console.log(formatReport(report));
  } else {
    console.error(`unknown command ${kind}`);
    process.exitCode = 2;
  }
}

const [kind = "", ...flags] = process.argv.slice(2);
await run(kind, Object.fromEntries(flags.map((f) => f.replace(/^--/, "").split("="))));
