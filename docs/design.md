# Atlas — design

A visual, agent-first code browser. It shows a repository at the level where the
thinking happens: places where data rests, the arrows of code between them, base
types and their subtypes, functions as morphisms `f: A → B`. Click a block, see the
code. The gitgraph stays. Editing does not.

Atlas is designed around one small algebra, and Atlas itself is the first subject
that has to obey it. Decisions are marked **Decision** with their reason; open
questions are at the end.

## 0. The shared language

Atlas exists so that a human and an agent can build software together at a level
where the human's sensors (a picture, a name, a shape that looks wrong) and the
agent's (types, calls, effects, everything at once) meet. Informal language is the
bottleneck on both sides. The shared language is a small piece of mathematics with
one picture per construct, and the two are the same structure, so that pointing at a
box and saying "this should not exist" is a statement both can act on.

- **Objects are places where data rests.** Memory(type), File(path), Database(table),
  Network(url), Process(command), Screen(human), Library(what a dependency produces).
  Creating an object is an arrow into Memory(T), the same kind of thing as writing a
  file. Every place carries a *what*: the type, the path, the table, the command.
- **Morphisms are functions.** A function's domain is its parameters plus what its
  body consumes (what it instantiates, what its callees hand back, what it reads);
  its codomain is what it returns plus what it writes, sends or runs. The author's
  names are the labels: `repo: Path`, `writes html → out`.
- **Composition is call order.** A route is a story: an entry point and its steps.
  Flows start at an entry and end at a place; both must be visible.
- **Zoom is a quotient on types.** Three hierarchies exist in code and give three
  consistent zooms: by base class (`Language ⊇ {Python, Rust}`), by composition
  (`Graph ⊇ {Node, Edge, Architecture}`), by module. A level is a choice of
  equivalence; zooming out coarsens it, zooming in refines one family at a time. The
  picture stays the same picture, only the boxes split or merge.
- **Filters are restrictions.** A route, an entry point, an edge kind, a commit range.
  Restriction and quotient do not commute; the order in a lens is meaning.
- **Two levels are never merged.** A step's own morphism (its signature, its direct
  effects) and what it transitively reaches are different quantities and are drawn
  differently.

Everything below is the engineering of that language.

## 1. The algebra

Four objects, four kinds of arrows.

```
Repo ──parse──▶ Graph ──lens──▶ Graph ──layout──▶ Picture
                  │
                  └──inspect──▶ Source
```

**Repo** is a directory at a commit. **Graph** is the one central type: typed nodes
(modules, classes, functions, artifacts, resting places) and typed edges. Everything
between parsing and drawing is `Graph → Graph`. **Picture** is geometry: positions,
shapes, labels. **Source** is a span of text.

**parse: Repo → Graph.** Per language, per file; a repo is the union of its files'
parses, plus what the repo declares about itself (scripts, Makefile targets,
notebooks). *Decision: the engine may know languages and libraries, never a
project.* Knowing what a `requests.get` call means is the engine's job. Knowing that
`rkg.sinks` is a sink is the project's job, declared in its own `architecture.yml`.
Nothing about rkg, vectormesh or Atlas is hard-coded, so that Atlas cannot flatter
the repos it was built on.

**lens: Graph → Graph.** The whole middle of Atlas. Lenses compose, so a view spec is
a list of lenses, and the order is meaning: restriction and quotient do not commute
in general. Two families and nothing else:

- **Restrictions** keep a subgraph, the inverse image of a predicate (a pullback):
  reachable from this entry point, hide private, hide tests, k-hop focus, only these
  edge kinds, only this commit range.
- **Quotients** identify nodes (a pushout, a coequalizer): by declared role, by base
  class, by package, by effect signature, by artifact. A quotient carries a
  refinement order, so "zoom in" is replacing a quotient by a finer one and "zoom
  out" by a coarser one. The quotients of a graph form a lattice; browsing is
  walking it.

**layout: Graph → Picture.** The only place geometry exists: columns, outline,
inheritance forest, layered flow. Nothing upstream knows about pixels.

**inspect: Node → Source.** The span of the symbol, read-only.

### The plaat is a functor image

The "simple story" of a codebase (in rkg: OZON API → assets on disk → `Graph` →
Memgraph and DuckDB → repository objects → HTTP) is not made of modules. Its boxes
are **resting places**: where data sits between arrows. The code is the arrows.

Take the small category **W** of resting places: the web, artifacts on disk,
databases, hub types in memory, the browser. The **effect signature** is a functor
`F: Graph → W` that sends each function to the arrow from what it takes in to what
it hands on. The high-level picture is the image of `F`. Refining **W** (Disk into
named artifacts, DB into Memgraph and DuckDB, `Sink` into its subclasses) refines
the picture; that is the zoom. The declared architecture is an override on the
naming of the classes, never the source of the picture.

Three things make `F` honest, and each is an engine task:

1. Database reads and writes are different arrows. A graph database's direction is what
   the Python code says to its driver: a managed transaction (`execute_read`,
   `execute_write`), a routing or an access mode; what runs inside it goes the same way.
   A graph query's text is not read, and where the code says nothing the direction stays
   open. Python's own database interface (PEP 249) has only `execute(sql)`, so SQL is
   read, by its grammar and in its database's dialect: a statement writes the table it
   changes and reads the tables it takes from (`INSERT INTO a SELECT .. FROM b`), a
   setting names no table, and a table filled in at run time is not named.
2. In-place mutation hides arrows: a function that takes a hub type and calls a
   mutator on it is an endomorphism `Graph → Graph`, not `Graph → ()`.
3. Disk is too coarse: `.env` and `assets/regelingen` are different resting places.
   Artifacts are named by path pattern, and they connect the route that writes them
   to the route that reads them.

Where the composition happens outside the package (vectormesh wires `VectorCache →
Serial → Trainer` in scripts and notebooks), the resting places are types, the
arrows are constructor calls, and a notebook is a route read cell by cell.

### Why look at code

Four questions; each view is a lens plus a layout that answers one of them. A view
that answers none is decoration.

| Question | Lens | Layout |
|---|---|---|
| **Orient** — what does this do? | restrict to a route, quotient by effect signature | story outline, plaat |
| **Verify** — does it do what I designed? | quotient by declared role, diff against the declaration | plaat with conformance marks |
| **Place** — where does new work land? | quotient by base class, k-hop focus | type lattice, architecture columns |
| **Explain** — to a student or an agent | any of the above, saved as a lens | the same, with notes |

### Atlas as its own subject

If Atlas obeys the algebra, its own story is `parse → lens → layout` with nothing
else on the route, and Atlas can check that about itself. In Rust that maps
directly: an enum per node and edge kind, a `Lens` trait with composition, and a
conformance check on the Atlas repo in its own CI. If the picture of Atlas in Atlas
is not four boxes, the code has drifted, not the picture.

## 2. Framework

**Decision: a Rust engine with a TypeScript/SVG frontend, shipped as one binary.
Not Mac-native, not Python.** Students must be able to run it with one command on
any OS; the engine must be typed and fast with a dependency story that needs no
virtualenv; tree-sitter is Rust-native; the algebra above is a set of algebraic
data types, which Rust expresses directly. Python is the spike language only.
Runner-up: Go (single binary, gonum; but tree-sitter is a cgo binding and graph
transforms without sum types get verbose). Rejected: Julia (no tree-sitter binding,
slow start, niche), TypeScript end-to-end (the engine core is better typed and
faster in Rust). A native shell (Tauri) can wrap the same code later.

| Layer | Choice | Why |
|---|---|---|
| Engine | Rust: `tree-sitter` crate, `petgraph`, `serde`, `ts-rs` | Grammars compile in; SCC, dominators and paths from petgraph; quotients, restrictions, effect signatures and conformance are small enough to own. `ts-rs` generates the TypeScript types so the graph model and the lens spec are defined once. |
| Server | `axum` + websocket, `notify` | JSON endpoints (`/graph`, `/lens/{name}`, `/source`, `/git/log`), live reload when a file changes, and the same process serves the MCP endpoint (`rmcp`, the official Rust MCP SDK). |
| Frontend | Vite, TypeScript, React 19, SVG | Boring and deep: the agent is most fluent in it. SVG (not canvas) so every element is CSS-styleable and design rounds are cheap. Canvas fallback only if a view exceeds ~2k elements. |
| Layout | elkjs (layered / Sugiyama) in a web worker | The flows are DAG-shaped; layered layout is what a reader expects. Columns and outlines are small custom layouts; ELK handles layered flows. |
| Code view | Shiki (read-only) | Editor-grade highlighting without an editor. "Open in editor" is a deep link (`vscode://`, `zed://`). |
| Design | one `tokens.css` | Role hue, kind shape, edge style, type scale, spacing, light/dark. A design round changes tokens first, components second. |
| Distribution | one static binary, frontend embedded (`rust-embed`) | `cargo install atlas` or a Homebrew tap, then `atlas serve .` |

Repository layout, which is also the plaat of Atlas:

```
atlas/
  engine/      Rust crate: parse · lens (restrict, quotient) · effects · git
  server/      Rust crate: axum app, MCP server, embeds frontend/dist
  cli/         Rust binary: analyze · serve · check · mcp
  frontend/    Vite + React: layouts, inspector, tokens.css; types generated by ts-rs
  spike/       throwaway experiments (M0, Python)
  docs/
```

## 3. The graph

**Nodes** — `module`, `class`, `function`, `method`, `external`, `artifact`,
`place` (a resting place in **W**); later `type_alias`, `constant`, `route`,
`command`, `cell`. Fields: `id` (stable), `kind`, `name`, `file`, `span`, `doc`,
`signature`, `decorators`, `role`, `role_source`, `entry`, `boundaries`.

**Edges** — `contains`, `imports`, `inherits`, `calls`, `instantiates`, `takes`,
`returns`, `has`, `reads`, `writes`, `mutates`, `variant`, `requests`; later `yields`,
`raises`, `decorates`.

**Entry points** — CLI commands, API routes, MCP tools (by decorator),
`[project.scripts]`, Makefile targets (recipes resolved to the commands they run),
scripts next to the package and in `scripts/`, `examples/`, `demo/`, notebooks, and what
a package exports (`__all__` or the public names its root `__init__.py` binds, and the
`__all__` of each public subpackage, as `components.Serial`; the `pub` items at the root
of a library crate). A re-exported name is followed to where it is defined. Each is the
start of a route; an exported class is a route whose steps are its public methods.

**Parts** — a repository is one or more parts: what one manifest roots (`Cargo.toml`,
`pyproject.toml`, `package.json`), read in its own language; a workspace's crates are
parts of one root, and nothing under `tests/` or `node_modules/` is a part. The graph holds
them side by side and every node knows its part. Parts meet over HTTP: a web framework's
routes stand at their full path (through `app.mount(prefix, sub_app)`,
`include_router(router, prefix=…)`, `APIRouter(prefix=…)`), and a string literal in one
part that is the full path of a route another part serves is a `requests` edge from the
node that holds it to the handler. A literal that names one of its own part's routes is a
link within that part. In the views, parts are a zoom level like families: the overview
is one step per part (memory stays inside it; what it reads and writes in the world, and
the HTTP of the parts it requests, stand outside), a click opens a part into its routes,
and keeping to one part is a restriction, in which another part is the web place it is
reached through. `atlas check` reads each part in its own language.

**Architecture** — a small `architecture.yml` in the subject repo, an override on
naming and on what counts as a legitimate flow, never the source of the picture:

```yaml
order: [Source, Graph, Sink, Repository, Analysis, API]   # data flow, left to right
bands: {top: [CLI], bottom: [Schema, Config]}             # orchestration, foundation
rules:                                                    # package prefix → role
  rkg.source: Source
  rkg.sinks: Sink
base_roles:                                               # base class → role
  rkg.sinks.base.Sink: Sink
dependencies: against_flow                                # consumers call providers
allowed:                                                  # fine in both directions
  - [Sink, Repository]
```

`order` describes the *data* flow. Dependencies (imports, calls, types) normally
run against it, consumer → provider: `Graph → Source` is expected, `Source → Graph`
is backflow. `dependencies: with_flow` flips that for push-style code. Bands are
exempt: the orchestration band may depend on anything, anything may depend on the
foundation band. Role assignment is per class: a class whose base chain reaches a
`base_roles` root takes that role wherever it lives. Without a yml, roles are
inferred (one per subpackage, or per module in a flat package, columns ordered so
providers stand left of consumers) and the UI says so.

## 4. Analytics, as lenses

Each layer is a pure function of the layer below and can be tested alone.

**L0 Syntax (tree-sitter).** Per file, per language, `.scm` queries yield
definitions, signatures, imports, calls, annotations, decorators, docstrings,
string literals at boundary calls. A new language is new queries, not new code.

**L1 Resolution.** Names become node ids: imports (absolute, relative, re-exports, a
name given to another: `graaf = triple`), bases, and calls through the type of their
receiver. That type is composed from its parts, the way a typechecker reads it:

- a name has the type of its binding: a parameter's annotation, an assignment, a loop
  over a collection, `with .. as`, a module global;
- `a.b` has the type of `b` in the type of `a`, declared on its class or a base (an
  annotated field, `self.b = ..` in a method, a property);
- `f(..)` has the type `f` constructs or its annotation gives back, place by place for
  `-> tuple[A, B]`;
- `a or b` and `a if c else b` have the type of either.

A type is a class of the package, many of them (`list[Graph]`), a mapping to them, or
a value a library made (`requests.Session()`), whose calls are that library's. Every
signature is read before any body, so the order of the files does not matter. Output:
the symbol graph plus a resolution report (resolved, external, builtin, local,
unknown) so the quality has a number.

**L2 Effects.** Boundary tags on every call that leaves the package (web, disk in
and out, each database read or written, LLM, model, process), artifacts by path
pattern, hub types by arrow count, mutation detection, and the effect signature
`F: Graph → W` per function and per route. This is the functor behind the plaat.

**L3 Quotients.** By declared role, by base class, by package, by effect signature,
by artifact; each with its refinement order. The quotient graph aggregates edges per
class pair and kind. Conformance is the difference between the quotient by declared
role and the declared flow.

**L4 Restrictions and structure.** Reach from an entry point (dominator tree for
"what does this command touch"), k-hop focus, edge-kind filters, commit ranges; SCC
condensation for import cycles; betweenness for hubs; k-shortest paths between two
types for the signature flow; Leiden as the honest fallback when nothing is
declared.

**L5 Git.** Commit DAG; diff hunks × spans → symbols; churn and age per symbol; a
commit range is a restriction lens.

**L6 Agent.** An MCP server over the same engine: `search`, `neighbourhood`,
`effects`, `quotient`, `conformance_report`, `signature_paths`, `apply_lens`.
Lenses are JSON view specs, a list of restrictions and quotients plus a layout, that
the agent writes and the browser renders. Annotations are agent explanations stored
per symbol.

## 5. Visualisation

Every view answers one of the four questions. If a legend needs more than three
items, it is two views.

| View | Question | Picture |
|---|---|---|
| **Story** | Orient | Routes into the program as rows of steps in call order; the resting places as columns; a dot where a step, or anything it reaches, touches one. Each step shows its morphism and expands into its own steps. Overview: every route and what it reaches. |
| **Flowchart** | Orient, Verify | The image of the effect functor: resting places as boxes, quotiented arrows between them, named by base class or package when declared and by signature otherwise. Click a box to refine it one level. Conformance marks where the declared flow disagrees. |
| **Architecture** | Verify, Place | Role columns left → right; orchestration band above, foundation band below; modules as boxes with class/function chips; aggregated edges; backflow in the alert colour. |
| **Type lattice** | Place | Inheritance forest; external bases as ghosts; wide leaf families as a grid; badges for abstract methods and overrides. |
| **Signature flow** | Orient | Types as nodes, functions as arrows, ELK-layered; select two types and the composition paths light up. |
| **Git graph** | Verify | Commit DAG; click a commit or drag a range → overlay on any view. |
| **Inspector** | all | Read-only code with room for 100 columns, signature, docstring, in/out edges, annotation, open in editor. |

**Design language.** Role = hue (one muted palette, the same in every view). Kind =
shape (module: rounded box; class: box with a header strip; function: pill;
external: dashed ghost; resting place: a plain rectangle). Edge kind = line style.
Sans for labels, mono for identifiers. Light and dark from the same tokens. Motion
only when the focus changes, and short.

**Interaction.** Command-K search; keyboard navigation; breadcrumb; URL state so a
view can be linked from an issue; a folder picker to open a project; never more
than two clicks from the code.

## 6. The feedback loop

- **M0 (done):** a Python spike on rkg, vectormesh, goad_toolkit and notebooktester.
  One question per round; ten was too early.
- **M1:** the Rust skeleton, with Atlas itself as the first subject, rkg and
  vectormesh second and third. Friction goes straight to Linear.
- **Design rounds** are explicit issues with the label *Design*: three screenshots,
  one question, one decision.
- **Decisions** are recorded on the issue that made them, with the reason.

## 7. Open questions

1. Restriction and quotient do not commute. Is "restrict, then quotient" the only
   order a lens list may use, or do both orders have a reading?
2. Is the role the right unit for colour, or is the base type?
3. Editing: probably never. "Open in editor" as a deep link should be enough.
4. The browser as a resting place: a click in `apps/plusmin` is an arrow into an
   API route. Parts are joined where one names a route the other serves; the function
   that sends the request, when the path reaches it through configuration
   (`requireRestApiEndpoint("tables")`), needs data flow.
5. Analysis cache across runs: a `.atlas/` directory next to `.git`.
