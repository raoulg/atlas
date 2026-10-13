import type { Currency, RateTable } from "../model.js";
import { FileRates } from "./file.js";
import { HttpRates } from "./http.js";

/** Where exchange rates come from. */
export interface RateSource {
  /** The latest rates against `base`. */
  latest(base: Currency): Promise<RateTable>;
}

/** The rates the program uses: from the file when RATES_OFFLINE is set, else from the service. */
export function rateSource(): RateSource {
  return process.env.RATES_OFFLINE ? new FileRates() : new HttpRates();
}
