Route: `heldobj.run`
- File(data/a/item.txt): right, written only by the first `put` (run/1).
- File(data/b/item.txt): right, written only by the second `put` (run/3).
- The constructor calls `Store(..)` (run/0, run/2) touch no file.
design.md: "an attribute set in `__init__` from its parameters ... holds what the constructor's callers hand it"; "two parameters a call hands together stay together".
Observed (0.68.0): each `put` writes both files (own), and each constructor reaches both: 8 lines for 2. DEFECTS I and B (both touches have by=["heldobj.run"]: the engine records the function, not the call).
