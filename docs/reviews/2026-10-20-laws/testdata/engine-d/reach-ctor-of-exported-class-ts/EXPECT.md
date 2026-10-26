Route: `reach-ctor-of-exported-class-ts.run` (a library package: `main`/`exports`)
- `new Store("data/store.txt")` touches nothing: data/store.txt is not in this route.
- Screen: right (out).
Observed (0.68.0): data/store.txt mid, read and written by the constructor call run/0. DEFECT B in TypeScript.
