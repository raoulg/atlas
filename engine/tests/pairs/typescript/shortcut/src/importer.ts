import { readFileSync } from "node:fs";

/** The rows of a CSV file with a header row, each as an object keyed by the header's names. */
export function readExpenses(file: string): any[] {
  const [header, ...rows] = readFileSync(file, "utf8").trim().split("\n");
  const columns = header.split(",").map((c) => c.trim());
  return rows.map((row) => {
    const cells = row.split(",").map((cell) => cell.trim());
    return Object.fromEntries(columns.map((c, i) => [c, cells[i]]));
  });
}
