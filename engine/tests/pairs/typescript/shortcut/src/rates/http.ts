import type { Currency, RateTable } from "../model.js";
import type { RateSource } from "./source.js";

async function getJson(url: string) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: ${res.status} ${res.statusText}`);
  return res.json();
}

/** Rates from a rates service over HTTP. */
export class HttpRates implements RateSource {
  async latest(base: Currency): Promise<RateTable> {
    return getJson(`${process.env.RATES_URL}/latest?base=${base}`);
  }
}
