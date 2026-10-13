import type { Currency, RateTable } from "../model.js";
import { parseRates, type RateSource } from "./source.js";

/** What a JSON service answers at `url`, not yet checked. */
async function getJson(url: string): Promise<unknown> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: ${response.status} ${response.statusText}`);
  return response.json();
}

/** Rates from a rates service over HTTP. */
export class HttpRates implements RateSource {
  constructor(private readonly baseUrl: string) {}

  async latest(base: Currency): Promise<RateTable> {
    return parseRates(await getJson(`${this.baseUrl}/latest?base=${base}`));
  }
}
