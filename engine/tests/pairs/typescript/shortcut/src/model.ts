/** The currencies the ledger converts between. */
export type Currency = "EUR" | "USD" | "GBP";

/** One expense, as it was paid. */
export interface Expense {
  id: string;
  /** The day it was paid, as an ISO date. */
  date: string;
  description: string;
  category: string;
  amount: number;
  currency: Currency;
}

/** Exchange rates against one base currency, on one day. */
export interface RateTable {
  base: Currency;
  date: string;
  rates: Record<Currency, number>;
}

/** What the expenses of one category add up to. */
export interface ReportLine {
  category: string;
  count: number;
  amount: number;
}

/** The expenses per category, in the ledger's currency. */
export interface Report {
  currency: Currency;
  ratesOf: string;
  lines: ReportLine[];
  total: number;
}
