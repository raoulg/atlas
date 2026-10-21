Route: `stamped.run`, `save` closed.
- File(data/out.txt): right (out).
- Lines from the closed step `save` to out.txt: two lines, one own (solid: `save`'s own write_text) and one reached (dashed: `stamp`'s write_text). CLAUDE.md sentence 6: "A step's own morphism and what it transitively reaches are different quantities and are never merged."; mathematics.md rule 3: "Different channels between the same two boxes stay different lines."
Observed (0.68.0): one own line `n:stamped.run/0 > out.txt` marked x2. DEFECT H: imageOf's `add` ANDs the flags (viewer/index.html:1555) and counts both fine lines into one bundle (1582); addEdge does the same (2086).
