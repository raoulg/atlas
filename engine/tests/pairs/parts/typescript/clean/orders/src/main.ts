import { readFileSync, writeFileSync } from "node:fs";

function takeOrders(): string {
  return readFileSync("data/orders.csv", "utf8");
}

export function main(): void {
  writeFileSync("out/orders.txt", takeOrders());
}
