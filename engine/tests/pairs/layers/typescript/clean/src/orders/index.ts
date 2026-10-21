import { price } from "./lines.js";

export function total(n: number): number {
  return price(n) + 1;
}
