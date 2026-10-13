import { readFile } from "node:fs/promises";

import type { Currency, RateTable } from "../model.js";
import { parseRates, type RateSource } from "./source.js";

/** Rates saved in a JSON file, for working offline. */
export class FileRates implements RateSource {
  constructor(private readonly file: string) {}

  async latest(base: Currency): Promise<RateTable> {
    const table = parseRates(JSON.parse(await readFile(this.file, "utf8")));
    if (table.base !== base) throw new Error(`rates: ${this.file} holds rates against ${table.base}, not ${base}`);
    return table;
  }
}
