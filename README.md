# CodeBrowser

A visual, agent-first code browser. It shows a repository at the level where the
thinking happens: base types and their subtypes, functions as morphisms `f: A → B`,
and information flowing through a handful of architectural blocks. Click a block,
see the code. The gitgraph stays. Editing does not.

- Design: [docs/design.md](docs/design.md)
- Planning: Linear project *CodeBrowser — code at the level of types and flows*
- First subject: `ruimtelijke-knowledge-graph` (`Source → Graph → Sinks ↔ Repository → API`)

## Spike (M0)

A throwaway pipeline that answers "does the abstraction hold?" on a real repo.

```bash
uv run --python 3.13 --with "tree-sitter==0.25.2" --with "tree-sitter-python==0.25.0" \
  spike/analyze.py /path/to/src/rkg --roles spike/rkg.roles.json -o spike/rkg.graph.json
python3 spike/build.py spike/rkg.graph.json spike/rkg.roles.json spike/rkg.html
open spike/rkg.html
```

`analyze.py` parses the package with tree-sitter into a typed symbol graph
(modules, classes, functions; `imports`, `inherits`, `calls`, `instantiates`,
`takes`, `returns`, `has`) and assigns each symbol a role from `rkg.roles.json`.
`viewer.html` renders three views over that graph: architecture (role columns),
type lattice (inheritance forest) and signature flow (types as nodes, functions
as arrows), with a read-only inspector.

tree-sitter is pinned to 0.25.x: the 0.26.0 Python binding crashes in the
garbage collector on trees of this size.
