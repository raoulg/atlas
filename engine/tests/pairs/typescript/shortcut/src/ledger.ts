import { randomUUID } from "node:crypto";

import { settings } from "./config.js";
import { normalize } from "./expense.js";
import { EXPORTERS } from "./exporters/exporter.js";
import { readExpenses } from "./importer.js";
import type { Expense, Report } from "./model.js";
import type { RateSource } from "./rates/source.js";
import { summarize } from "./report.js";
import type { ExpenseStore } from "./store.js";

/** The expenses with what the ledger knows about them: where they are kept and what they are worth. */
export class Ledger {
  constructor(
    private readonly store: ExpenseStore,
    private readonly rates: RateSource,
  ) {}

  /** Keeps an expense, filed under one of the configured categories. */
  record(input: any): Expense {
    const expense: Expense = { ...input, id: randomUUID(), amount: Number(input.amount) };
    normalize(expense);
    const { categories } = settings();
    if (!categories.includes(expense.category)) {
      throw new Error(`unknown category ${expense.category}; known are ${categories.join(", ")}`);
    }
    this.store.add(expense);
    return expense;
  }

  /** Keeps every expense in a CSV file; gives how many there were. */
  importFile(file: string): number {
    const rows = readExpenses(file);
    rows.forEach((row) => this.record(row));
    return rows.length;
  }

  expenses(): Record<string, unknown>[] {
    return this.store.all();
  }

  async report(): Promise<Report> {
    const rates = await this.rates.latest(settings().currency);
    return summarize(this.store.all() as unknown as Expense[], rates);
  }

  async exportReport(report: Report, format: string): Promise<void> {
    await EXPORTERS[format](report);
  }
}
