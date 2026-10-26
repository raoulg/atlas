import { Failed, Fit } from "./model.js";

export function fit(xs: number[]): Fit | Failed {
  if (xs.length === 0) {
    return new Failed("no measurements");
  }
  return new Fit(xs.reduce((a, b) => a + b, 0) / xs.length);
}
