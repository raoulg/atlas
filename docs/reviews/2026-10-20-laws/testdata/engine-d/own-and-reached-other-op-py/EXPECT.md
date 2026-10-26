Route: `stampedop.run`, `save` closed.
- Lines from `save` to data/out.txt: one own (write_text) and one reached (`stamp`'s `open(.., "a")`), two lines.
Observed (0.68.0): one own line x2. DEFECT H (same as own-and-reached-same-file-py, with different operations).
