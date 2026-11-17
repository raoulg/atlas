import { readFileSync } from "node:fs";
import { save } from "notekit";

export function main(): void {
  const orders = readFileSync("data/orders.csv", "utf8");
  save("out/orders.txt", orders);
}
