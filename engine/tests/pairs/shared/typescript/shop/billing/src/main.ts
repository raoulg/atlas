import { readFileSync } from "node:fs";
import { save, write } from "notekit";
import { sync } from "notekit/disk.js";

export function main(): void {
  const bills = readFileSync("data/bills.csv", "utf8");
  save("out/bills.txt", bills);
  write("out/bills.log", "saved");
  sync("out/bills.log");
}
