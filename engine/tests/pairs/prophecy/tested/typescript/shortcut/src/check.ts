import { seal } from "./index.js";

/** Checks a report before it is kept. */
export function sealedTwice(r: string): string {
  return seal(seal(r));
}
