# Atlas

A visual, agent-first code browser. It shows a repository at the level where the
thinking happens: places where data rests, the arrows of code between them, base
types and their subtypes, functions as morphisms `f: A → B`. Click a block, see the
code. The gitgraph stays. Editing does not.

Atlas is one small algebra, and Atlas itself is the first subject that has to obey it:

```
Repo ──parse──▶ Graph ──lens──▶ Graph ──layout──▶ Picture
                  │
                  └──inspect──▶ Source
```

Lenses compose and come in two families: restrictions (subgraphs) and quotients
(identify nodes; refining a quotient is the zoom). The high-level picture of a
codebase is the image of an effect functor into the category of resting places. The
engine knows languages and libraries, never a project.

- Design: [docs/design.md](docs/design.md)
- Planning: Linear project *Atlas — code at the level of types and flows*
- Subjects: Atlas itself, then `ruimtelijke-knowledge-graph`, `vectormesh`, `goad_toolkit`, `notebooktester`

## Spike (M0)

A throwaway pipeline that answers "does the abstraction hold?" on a real repo.

```bash
uv run --python 3.13 --with "tree-sitter==0.25.2" --with "tree-sitter-python==0.25.0" \
  spike/analyze.py /path/to/src/rkg --roles spike/rkg.roles.json -o spike/rkg.graph.json
python3 spike/build.py spike/atlas.html spike/rkg.graph.json spike/vectormesh.graph.json
open spike/atlas.html
```

Without `--roles` the analyzer infers one role per subpackage (or per module in a flat
package) and orders the columns so providers stand left of their consumers. Every
graph passed to `build.py` becomes a project in the page's switcher.

`analyze.py` parses the package with tree-sitter into a typed symbol graph
(modules, classes, functions; `imports`, `inherits`, `calls`, `instantiates`,
`takes`, `returns`, `has`) and assigns each symbol a role from `rkg.roles.json`.
It also finds the routes into the program: Makefile targets, `[project.scripts]`,
CLI commands, API routes, MCP tools and top-level scripts, and tags every call that
leaves the package with the part of the world it touches (web, disk, databases,
LLM, models). `viewer.html` renders four views over that graph: story (routes in
call order against the world they touch), architecture (role columns), type
lattice (inheritance forest) and signature flow (types as nodes, functions as
arrows), with a read-only inspector.

tree-sitter is pinned to 0.25.x: the 0.26.0 Python binding crashes in the
garbage collector on trees of this size.
