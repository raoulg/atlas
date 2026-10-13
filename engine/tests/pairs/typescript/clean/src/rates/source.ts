import { fieldsOf, isCurrency, type Currency, type RateTable } from "../model.js";

/** Where exchange rates come from. */
export interface RateSource {
  /** The latest rates against `base`. */
  latest(base: Currency): Promise<RateTable>;
}

/** A rate table from what a rates service or file holds, checked. */
export function parseRates(raw: unknown): RateTable {
  const fields = fieldsOf(raw, "rates");
  const base = fields.get("base");
  const date = fields.get("date");
  const rates = fieldsOf(fields.get("rates"), "rates.rates");
  if (!isCurrency(base)) throw new Error(`rates: unknown base ${String(base)}`);
  if (typeof date !== "string") throw new Error("rates: date must be text");
  return { base, date, rates: { EUR: rate(rates, "EUR"), USD: rate(rates, "USD"), GBP: rate(rates, "GBP") } };
}

function rate(rates: ReadonlyMap<string, unknown>, currency: Currency): number {
  const r = rates.get(currency);
  if (typeof r !== "number" || r <= 0) throw new Error(`rates: no rate for ${currency}`);
  return r;
}
