import { serve } from "@hono/node-server";
import { Hono } from "hono";

import { loadConfig } from "./config.js";
import { parseExpense } from "./expense.js";
import { Ledger } from "./ledger.js";
import { CONFIG_FILE, DATABASE_FILE, PORT, RATES_URL } from "./places.js";
import { HttpRates } from "./rates/http.js";
import { ExpenseStore } from "./store.js";

const ledger = new Ledger(loadConfig(CONFIG_FILE), new ExpenseStore(DATABASE_FILE), new HttpRates(RATES_URL));

const app = new Hono();

app.get("/api/expenses", (c) => c.json(ledger.expenses()));

app.post("/api/expenses", async (c) => {
  const expense = ledger.record(parseExpense(await c.req.json()));
  return c.json(expense, 201);
});

app.get("/api/report", async (c) => c.json(await ledger.report()));

serve({ fetch: app.fetch, port: PORT });
