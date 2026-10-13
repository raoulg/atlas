import type { Currency, Expense, RateTable, Report, ReportLine } from "./model.js";

/** The expenses per category, converted into the currency the rates are against. */
export function summarize(expenses: readonly Expense[], rates: RateTable): Report {
  const lines = new Map<string, ReportLine>();
  for (const e of expenses) {
    const line = lines.get(e.category) ?? { category: e.category, count: 0, amount: 0 };
    lines.set(e.category, { ...line, count: line.count + 1, amount: line.amount + convert(e.amount, e.currency, rates) });
  }
  const sorted = [...lines.values()].sort((a, b) => b.amount - a.amount);
  return { currency: rates.base, ratesOf: rates.date, lines: sorted, total: sorted.reduce((sum, l) => sum + l.amount, 0) };
}

function convert(amount: number, from: Currency, rates: RateTable): number {
  return Math.round((amount / rates.rates[from]) * 100) / 100;
}

/** The report as lines of text, for a terminal. */
export function formatReport(report: Report): string {
  const rows = report.lines.map((l) => `${l.category.padEnd(16)} ${String(l.count).padStart(4)} ${l.amount.toFixed(2).padStart(10)}`);
  return [...rows, `${"total".padEnd(21)} ${report.total.toFixed(2).padStart(10)} ${report.currency}`, `rates of ${report.ratesOf}`].join("\n");
}
