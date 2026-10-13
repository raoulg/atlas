import type { Expense } from "./model.js";

/** Files the expense's category the way the ledger keeps it. */
export function normalize(expense: Expense): void {
  expense.category = expense.category.trim().toLowerCase();
}
