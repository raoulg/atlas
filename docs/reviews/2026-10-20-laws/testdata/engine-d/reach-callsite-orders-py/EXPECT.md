Route: `callsite.run`
- File(data/alpha.txt): right, by `alpha` only.
- File(data/beta.txt): both — `show_beta` reads it as it was and only then `beta` writes it ("A place the route reads and then writes on is in both").
Observed (0.68.0): beta.txt mid; `flow.arrows["callsite.run"].reads_back = ["w:File:data/beta.txt"]`, i.e. the engine says alpha wrote beta.txt before it was read. DEFECT N: orders (flow.rs:546-547) takes every touch of `write_to`, whatever caller makes its path whole (`by`), and memoizes it per function.
