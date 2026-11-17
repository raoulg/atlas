import { readFileSync } from "node:fs";
import { save } from "notekit";

export function main(): void {
  const bills = readFileSync("data/bills.csv", "utf8");
  save("out/bills.txt", bills);
}
