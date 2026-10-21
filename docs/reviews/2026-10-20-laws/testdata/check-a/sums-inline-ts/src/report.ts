import { Failed, Fit } from "./model.js";

export function report(r: Fit | Failed): string {
  return r instanceof Fit ? `fit ${r.score}` : `failed: ${r.reason}`;
}
