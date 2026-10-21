import { Fit, type Result } from "./model.js";

export function report(r: Result): string {
  return r instanceof Fit ? `fit ${r.score}` : `failed: ${r.reason}`;
}
