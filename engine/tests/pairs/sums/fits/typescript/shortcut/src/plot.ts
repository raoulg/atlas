import { Failed, Fit } from "./model.js";

export function points(r: Fit | Failed | undefined): number[] {
  return r instanceof Fit ? [r.score] : [];
}
