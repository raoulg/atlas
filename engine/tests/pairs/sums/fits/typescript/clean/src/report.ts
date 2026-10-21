import { Failed, Fit } from "./model.js";

export interface Report {
  line: string;
  points: number[];
}

export function describe(r: Fit | Failed): Report {
  if (r instanceof Fit) {
    return { line: `fit ${r.score.toFixed(2)}`, points: [r.score] };
  }
  return { line: `failed: ${r.reason}`, points: [] };
}
