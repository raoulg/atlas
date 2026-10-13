import { readFile } from "node:fs/promises";

import type { Currency, RateTable } from "../model.js";
import type { RateSource } from "./source.js";

/** Rates saved in a JSON file, for working offline. */
export class FileRates implements RateSource {
  async latest(base: Currency): Promise<RateTable> {
    const file = process.env.RATES_FILE ?? "data/rates.json";
    const table: RateTable = JSON.parse(await readFile(file, "utf8"));
    if (table.base !== base) throw new Error(`rates: ${file} holds rates against ${table.base}, not ${base}`);
    return table;
  }
}
