import { Failed, Fit } from "./model.js";

export function line(r: Fit | Failed): string {
  return r instanceof Fit ? `fit ${r.score.toFixed(2)}` : `failed: ${r.reason}`;
}
