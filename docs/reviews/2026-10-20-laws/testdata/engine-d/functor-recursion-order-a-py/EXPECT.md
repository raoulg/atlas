Route: `recura.run`
- File(data/log.txt): both — `show` reads the log as it was, then `save` writes on it: "A place the route reads and then writes on is in both". Must be the same as functor-recursion-order-b-py, which only orders the two definitions otherwise.
Observed (0.68.0): mid (no `updates` on run or show). DEFECT P: in a recursive cycle the first function worked out (graph order) gets its caller's empty, in-progress map and is memoized with it (flow.rs:533-536, 599): the arrow depends on the order of definitions.
