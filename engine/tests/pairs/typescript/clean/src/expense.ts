import { randomUUID } from "node:crypto";

import { fieldsOf, isCurrency, type Expense } from "./model.js";

const DATE = /^\d{4}-\d{2}-\d{2}$/;

/** A new expense from what a user or a file handed in, with an id of its own. */
export function parseExpense(raw: unknown): Expense {
  return expenseFrom(fieldsOf(raw, "expense"), randomUUID());
}

/** An expense kept earlier, under the id it was kept by. */
export function storedExpense(raw: unknown): Expense {
  const fields = fieldsOf(raw, "stored expense");
  const id = fields.get("id");
  if (typeof id !== "string") throw new Error("stored expense: no id");
  return expenseFrom(fields, id);
}

function expenseFrom(fields: ReadonlyMap<string, unknown>, id: string): Expense {
  const date = fields.get("date");
  const description = fields.get("description");
  const category = fields.get("category");
  const amount = fields.get("amount");
  const currency = fields.get("currency");
  if (typeof date !== "string" || !DATE.test(date)) throw new Error(`expense: bad date ${String(date)}`);
  if (typeof description !== "string") throw new Error("expense: description must be text");
  if (typeof category !== "string" || category.trim() === "") throw new Error("expense: category must be text");
  if (typeof amount !== "number" || !Number.isFinite(amount)) throw new Error(`expense: bad amount ${String(amount)}`);
  if (!isCurrency(currency)) throw new Error(`expense: unknown currency ${String(currency)}`);
  return { id, date, description, category, amount, currency };
}

/** The expense with its category written the way the ledger files it. */
export function normalize(expense: Expense): Expense {
  return { ...expense, category: expense.category.trim().toLowerCase() };
}
