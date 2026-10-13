import { readFileSync } from "node:fs";

import { parseExpense } from "./expense.js";
import type { Expense } from "./model.js";

const COLUMNS = ["date", "description", "category", "amount", "currency"] as const;

/** The expenses in a CSV file with a header row: date, description, category, amount, currency. */
export function readExpenses(file: string): Expense[] {
  const [header, ...rows] = readFileSync(file, "utf8").trim().split("\n");
  if (header?.trim() !== COLUMNS.join(",")) throw new Error(`${file}: expected the columns ${COLUMNS.join(",")}`);
  return rows.map((row) => parseExpense(fromRow(row)));
}

function fromRow(row: string): Record<(typeof COLUMNS)[number], string | number> {
  const [date = "", description = "", category = "", amount = "", currency = ""] = row.split(",").map((cell) => cell.trim());
  return { date, description, category, amount: Number(amount), currency };
}
