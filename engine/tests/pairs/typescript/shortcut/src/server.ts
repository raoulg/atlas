import { serve } from "@hono/node-server";
import { Hono } from "hono";

import { configure, loadConfig } from "./config.js";
import { Ledger } from "./ledger.js";
import { rateSource } from "./rates/source.js";
import { ExpenseStore } from "./store.js";

configure(loadConfig(process.env.CONFIG_FILE ?? "config/expenses.json"));
const ledger = new Ledger(new ExpenseStore(), rateSource());

const app = new Hono();

app.get("/api/expenses", (c) => c.json(ledger.expenses()));

app.post("/api/expenses", async (c) => c.json(ledger.record(await c.req.json()), 201));

app.get("/api/report", async (c) => c.json(await ledger.report()));

serve({ fetch: app.fetch, port: Number(process.env.PORT ?? 8080) });
