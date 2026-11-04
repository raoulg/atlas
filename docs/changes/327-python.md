---
issue: 327
title: A database is named by its file however its path is built
area: engine
commit: 8feac9b0
pictures: []
---
A database opened on a file is named by that file's path, as any file is, however the code builds the path. Before,
`sqlite3.connect(os.path.join("data", kind, "counts.db"))` was named `data`, the same path glued as text `data/`, and
a function that opened two databases (`counts.db` and `totals.db`) was read as opening one; a database a type keeps
was named by its attribute (`self.path`) rather than by the file. Now the joined and the glued path, in Python and in
Rust, give the same file, and two files are two.

```text
example: engine/tests/pairs/opened/{python,rust}/{clean,shortcut}, what both(kind) opens
  0.71.0 (9cce2767)   python clean: data · python shortcut: data/ · rust clean: data
                      rust shortcut: data/{kind}/counts.db, data/{}
  with #327           every side: data/{kind}/counts.db, data/totals.db
example: engine/tests/pairs/python/clean, ReadingStore.connect
  0.71.0 (9cce2767)   self.path
  with #327           data/harvest.db
```
