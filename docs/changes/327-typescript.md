---
issue: 327
title: TypeScript reads entries, a table's keys and the environment's variables as the other languages do
area: engine
commit: 8feac9b0
pictures: []
---
Three forms TypeScript programs write are read as their Python and Rust forms are. A listing with
`{ withFileTypes: true }` gives entries whose `name` is the name below the folder listed, so
`join(DATA, e.name, "meta.json")` is `data/*/meta.json`, as `os.path.join(DATA, e.name, ...)` and
`Path::new(DATA).join(e.file_name())` now are too. A loop `for (const k in KINDS)` goes over the keys the table
writes, as Python's `for k in KINDS` and `KINDS.keys()` do: each key is a file of its own. `process.env[STORE]` with a
constant `STORE = "STORE"`, and `process.env["STORE"]`, are the variable `STORE`, as `process.env.STORE` is; a name no
code decides stays unnamed.

```text
example: four functions of one TypeScript module (scratch), the files each reads
  entries   readdirSync(DATA, { withFileTypes: true }) → join(DATA, e.name, "meta.json")
  kinds     for (const k in KINDS) → join(DATA, k, "index.json"), KINDS = { plots: 1, tables: 2 }
  keyed     process.env[STORE]!, STORE = "STORE"
  member    process.env.STORE!
0.71.0 (9cce2767)
  entries   data/{name}/meta.json
  kinds     data/{k}/index.json
  keyed     process.env[STORE]!
  member    {$STORE}
with #327
  entries   data/*/meta.json
  kinds     data/plots/index.json, data/tables/index.json
  keyed     {$STORE}
  member    {$STORE}
```
