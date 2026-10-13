import { readFileSync } from "node:fs";

import type { Currency } from "./model.js";

/** What the ledger is set up with. */
export interface Config {
  /** The currency reports are made in. */
  currency: Currency;
  /** The categories an expense may be filed under. */
  categories: string[];
}

let current: Config = { currency: "EUR", categories: [] };

/** The configuration in a JSON file. */
export function loadConfig(file: string): Config {
  const config = JSON.parse(readFileSync(file, "utf8")) as Config;
  if (!config.currency) throw new Error(`${file}: no currency`);
  return config;
}

/** Makes `config` the configuration the whole program uses from now on. */
export function configure(config: Config): void {
  current = config;
}

export function settings(): Config {
  return current;
}
