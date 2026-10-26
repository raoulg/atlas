import type { Result } from "./model.js";

export function fit(xs: number[]): Result {
  return xs.length > 0 ? { kind: "fit", score: xs.reduce((a, b) => a + b, 0) } : { kind: "failed", reason: "empty" };
}
