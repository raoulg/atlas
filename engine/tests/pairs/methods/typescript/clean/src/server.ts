// One handler for each method the path serves: a GET reads the notes, a POST keeps one.
import express from "express";
import { readFileSync, writeFileSync } from "node:fs";

const NOTES = "data/notes.json";

const app = express();

app.get("/api/notes", (req, res) => {
  res.json(JSON.parse(readFileSync(NOTES, "utf8")));
});

app.post("/api/notes", (req, res) => {
  writeFileSync(NOTES, JSON.stringify(req.body));
  res.status(201).end();
});

app.listen(8080);
