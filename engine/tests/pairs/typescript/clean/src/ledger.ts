import type { Config } from "./config.js";
import { normalize } from "./expense.js";
import type { ReportExporter } from "./exporters/exporter.js";
import { readExpenses } from "./importer.js";
import type { Expense, Report } from "./model.js";
import type { RateSource } from "./rates/source.js";
import { summarize } from "./report.js";
import type { ExpenseStore } from "./store.js";

/** The expenses with what the ledger knows about them: where they are kept and what they are worth. */
export class Ledger {
  constructor(
    private readonly config: Config,
    private readonly store: ExpenseStore,
    private readonly rates: RateSource,
  ) {}

  /** Keeps an expense, filed under one of the configured categories. */
  record(expense: Expense): Expense {
    const filed = normalize(expense);
    if (!this.config.categories.includes(filed.category)) {
      throw new Error(`unknown category ${filed.category}; known are ${this.config.categories.join(", ")}`);
    }
    this.store.add(filed);
    return filed;
  }

  /** Keeps every expense in a CSV file; gives how many there were. */
  importFile(file: string): number {
    const expenses = readExpenses(file).map((e) => this.record(e));
    return expenses.length;
  }

  expenses(): Expense[] {
    return this.store.all();
  }

  async report(): Promise<Report> {
    const rates = await this.rates.latest(this.config.currency);
    return summarize(this.store.all(), rates);
  }

  async exportReport(exporter: ReportExporter, file: string): Promise<void> {
    const report = await this.report();
    await exporter.write(report, file);
  }
}
