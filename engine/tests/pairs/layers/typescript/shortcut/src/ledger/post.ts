import { round } from "../money/round.js";

export function post(n: number): number {
  return round(n) + 1;
}
