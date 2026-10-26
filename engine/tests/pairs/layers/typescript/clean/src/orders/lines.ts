import { tax } from "./tax.js";

export function price(n: number): number {
  return n + tax(n);
}
