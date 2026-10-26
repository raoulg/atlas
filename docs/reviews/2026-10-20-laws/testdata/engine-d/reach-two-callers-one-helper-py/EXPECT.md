Route: `twocallers.run`
- File(data/alpha.txt): right, reached only by `alpha` (run/0).
- File(data/beta.txt): right, reached only by `beta` (run/1).
Rule 5: "What a step reaches is the closure of its calls in the fine graph ... never read off a quotient."
Observed (0.68.0): alpha and beta are one pill `run/0+run/1` reaching both files; opened, the occurrence `run/0/0` (write_to under alpha) writes both files as its own. DEFECT I: touchesIn/inRoute (viewer/index.html:773-777) keep a touch when the route reaches its `by` function, not when the occurrence's path does; reach is memoized per function (754).
