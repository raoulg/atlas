import { Failed, Fit, type Result } from "./model.js";

export function fit(xs: number[]): Result {
  return xs.length > 0 ? new Fit(xs.reduce((a, b) => a + b, 0)) : new Failed("empty");
}
