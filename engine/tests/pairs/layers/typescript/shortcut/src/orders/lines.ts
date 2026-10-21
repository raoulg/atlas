import { tax } from "./index.js";

export function price(n: number): number {
  return n + tax(n);
}
