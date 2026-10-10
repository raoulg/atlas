import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { Hono } from "hono";
import Anthropic from "@anthropic-ai/sdk";
import { zodOutputFormat } from "@anthropic-ai/sdk/helpers/zod";
import { Pool } from "pg";

const pool = new Pool();
const run = promisify(execFile);

export async function vat(tekst: string): Promise<string> {
  const client = new Anthropic();
  const antwoord = await client.messages.create({ model: "claude", max_tokens: 100, messages: [{ role: "user", content: tekst }], format: zodOutputFormat(null) });
  return String(antwoord.id);
}

export const regels = new Hono()
  .get("/:id", async (c) => {
    const rows = await pool.query("SELECT * FROM regels WHERE id = $1", [c.req.param("id")]);
    return c.json(rows);
  })
  .post("/", async (c) => {
    await pool.query("INSERT INTO regels (tekst) VALUES ($1)", [await vat("x")]);
    await run("pdflatex", ["regels.tex"]);
    return c.body(null, 201);
  });
