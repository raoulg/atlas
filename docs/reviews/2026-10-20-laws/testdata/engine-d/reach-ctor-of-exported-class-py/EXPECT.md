Route: `storebuild.run`
- `Store("data/store.txt")` touches nothing: data/store.txt is not in this route (save/load are never called).
- Screen: right (out).
Observed (0.68.0): data/store.txt mid, with lines store.txt -> run/0 and run/0 -> store.txt from the constructor call. DEFECT B: for an exported class, body_of (flow.rs:909-916) is its constructor and its public methods, and the viewer's reach of the cut class step (viewer/index.html:1059, 1381, 754-768) walks that body.
