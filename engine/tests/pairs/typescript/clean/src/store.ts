import Database from "better-sqlite3";

import { storedExpense } from "./expense.js";
import type { Expense } from "./model.js";

const SCHEMA = `CREATE TABLE IF NOT EXISTS expenses (
  id TEXT PRIMARY KEY,
  date TEXT NOT NULL,
  description TEXT NOT NULL,
  category TEXT NOT NULL,
  amount REAL NOT NULL,
  currency TEXT NOT NULL
)`;

const INSERT = "INSERT INTO expenses (id, date, description, category, amount, currency) VALUES (?, ?, ?, ?, ?, ?)";
const SELECT_ALL = "SELECT id, date, description, category, amount, currency FROM expenses ORDER BY date";

/** The expenses, kept in a SQLite database. */
export class ExpenseStore {
  private readonly db: Database.Database;

  constructor(file: string) {
    this.db = new Database(file);
    this.db.exec(SCHEMA);
  }

  add(expense: Expense): void {
    this.db.prepare(INSERT).run(expense.id, expense.date, expense.description, expense.category, expense.amount, expense.currency);
  }

  all(): Expense[] {
    return this.db.prepare(SELECT_ALL).all().map(storedExpense);
  }
}
