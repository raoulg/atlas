import { readFileSync } from "node:fs";

import { fieldsOf, isCurrency, type Currency } from "./model.js";

/** What the ledger is set up with. */
export interface Config {
  /** The currency reports are made in. */
  currency: Currency;
  /** The categories an expense may be filed under. */
  categories: string[];
}

/** The configuration in a JSON file. */
export function loadConfig(file: string): Config {
  return parseConfig(JSON.parse(readFileSync(file, "utf8")));
}

/** A configuration from what a JSON file holds, checked. */
export function parseConfig(raw: unknown): Config {
  const fields = fieldsOf(raw, "config");
  const currency = fields.get("currency");
  const categories = fields.get("categories");
  if (!isCurrency(currency)) throw new Error(`config: unknown currency ${String(currency)}`);
  if (!Array.isArray(categories) || !categories.every((c): c is string => typeof c === "string")) {
    throw new Error("config: categories must be a list of names");
  }
  return { currency, categories };
}
