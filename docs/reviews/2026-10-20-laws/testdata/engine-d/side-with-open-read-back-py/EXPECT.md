Route: `withopen.run`
- File(data/note.txt): between (mid) — `save` writes it, `load` reads it with `with open(...) as f` and returns `f.read()`: used, "A place one step writes and a later step reads and uses ... stands between them."
- Screen: right (out).
Observed (0.68.0): out; the read touch carries `checks`. DEFECT E: Python's UseSyntax (python.rs:3218-3230) binds `=`, `+=`, `for`, `:=` but not `with ... as`, so a file opened in a `with` is never followed and every such read is a check.
