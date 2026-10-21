Route: `loopnote.run`
- File(data/note.txt): both — the read of one word finds the note as it was and a later word writes on it: "A place the route reads and then writes on is in both". The write is in an arm *and* after a read (next iteration), so the make-sure-of case ("writes only before it reads there") does not apply.
Observed (0.68.0): mid. DEFECT C: orders (flow.rs:546-558) sorts events by line; a loop's next iteration is not seen.
