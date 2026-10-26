Route: `mergedorder.run`
- File(data/note.txt): both — when not seeded the read finds the note as it was, and the last line always writes on it: "A place the route reads and then writes on is in both".
Observed (0.68.0): mid. DEFECT G: the two `write_text` calls are one touch (builder.rs:155-167 merges equal touches and keeps the first line, 11, in the arm), so orders (flow.rs:546) never sees the write after the read.
