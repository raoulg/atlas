/** The currencies the ledger converts between. */
export type Currency = "EUR" | "USD" | "GBP";

const CURRENCIES: readonly Currency[] = ["EUR", "USD", "GBP"];

export function isCurrency(value: unknown): value is Currency {
  return CURRENCIES.some((c) => c === value);
}

/** The fields of a JSON object, each still to be checked; what is no object is an error. */
export function fieldsOf(raw: unknown, what: string): ReadonlyMap<string, unknown> {
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) {
    throw new Error(`${what}: expected an object`);
  }
  return new Map(Object.entries(raw));
}

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
