import { Hono } from "hono";
import { serve } from "@hono/node-server";
import { regels } from "./regels";

const app = new Hono();

app.get("/api/health", (c) => c.json({ ok: true }));
app.route("/api/regels", regels);

serve({ fetch: app.fetch, port: 3000 });
