import { price } from "./lines.js";

export function total(n: number): number {
  return price(n) + 1;
}

export function tax(n: number): number {
  return Math.floor(n / 5);
}
