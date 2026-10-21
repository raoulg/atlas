Route: `dispatch.run`
- File(data/store.txt): right (out) — `work` saves, then only verifies (a check), through `Store`, which the route hands a `FileStore`: "What the route reads back of what it wrote, or reads only to check or count, it does not use: that place stays in the codomain."
Observed (0.68.0): mid; `work` and `run` have no `reads_back`. DEFECT M: orders (flow.rs:549, 569) follows the call's static target (`Store.save`, no touches), never the override the body runs (body_of/method_on, flow.rs:879-903; the viewer's reach follows it).
Compare functor-direct-readback-py (the same without the base class): out.
