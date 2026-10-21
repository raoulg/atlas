Route: `globbed.run`
- The listing and the reads of `m`: one place, the fiber `File(assets/regelingen/*/IO-*)`, left — mathematics.md: "A place on the disk the code names only through a variable is drawn as the term it evaluates to (`assets/regelingen/*/IO-*`), not as the variable (`m`)."
Observed (0.68.0): two places: the glob at File(assets/regelingen/*/IO-*), the reads of `m` at File(assets/regelingen/). DEFECT L: paths::place keeps a pattern only for the glob operation itself (paths.rs:328-339); any other operation on the same term rests in the directory before its first unknown part.
