// One handler for every method at the path, which looks at the method itself.
import express from "express";
import { readFileSync, writeFileSync } from "node:fs";

const NOTES = "data/notes.json";

const app = express();

app.all("/api/notes", (req, res) => {
  if (req.method === "POST") {
    writeFileSync(NOTES, JSON.stringify(req.body));
    res.status(201).end();
  } else {
    res.json(JSON.parse(readFileSync(NOTES, "utf8")));
  }
});

app.listen(8080);
