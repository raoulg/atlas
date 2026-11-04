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
  file. A value a module keeps between calls, which its functions change, is a place too,
  Global(module.name): written by whoever changes it, read by whoever uses it, drawn and
  not judged. A module, a class and a function are code, not values: what a library's module
  holds is the place (`Global(os.environ)`, not `Global(os)`). Every place carries a *what*: the type, the path, the table, the command. The
  screen is the terminal both ways: a print, a stream or a terminal window shows the
  human something, and `input`, a prompt, a key or standard input reads what they type,
  in every language alike; a print handed a file writes the file, and a logging library
  writes where it is configured to, so neither is the screen.
- **Morphisms are functions.** A function's domain is its parameters plus what its
  body consumes (what it instantiates, what its callees hand back, what it reads);
  its codomain is what it returns plus what it writes, sends or runs. The author's
  names are the labels: `repo: Path`, `writes html → out`.
- **Composition is call order.** A route is a story: an entry point and its steps.
  Flows start at an entry and end at a place; both must be visible.
- **Zoom is a quotient on types.** Three hierarchies exist in code and give three
  consistent zooms: by base class (`Language ⊇ {Python, Rust}`), by composition
  (`Graph ⊇ {Node, Edge, Part}`), by module. A level is a choice of
  equivalence; zooming out coarsens it, zooming in refines one family at a time. The
  picture stays the same picture, only the boxes split or merge.
- **Filters are fibers.** A route, an entry point, an edge kind, a commit range: the
  part of the graph where a predicate holds. Zoom and filter come from one map: the
  quotient by `f: G → B` is the overview, the fiber over one `b` is that `b` alone, and
  opening `b` puts its fiber in its place. Fiber and quotient do not commute; the order
  in a view is meaning.
- **Two levels are never merged.** A step's own morphism (its signature, its direct
  effects) and what it transitively reaches are different quantities and are drawn
  differently.

Everything below is the engineering of that language.

## 1. The algebra

Five objects, five kinds of arrows.

```
Disk ──read──▶ Repo ──parse──▶ Graph ──F──▶ Flow ──view──▶ Flow ──layout──▶ Picture
                                   │
                                   └──inspect──▶ Source
```

**Flow** is the image of the effect functor `F` (below): what every step of every body runs, in
call order and with dispatch (`bodies`), what every function takes and gives as types
(`arrows`), how the types group into families and which are doors (`types`); each touch says
where it rests, which side of its step it stands on (in, out, or open where the code does not
say) and what it does there. An arrow also names the places it reads only back, what it wrote
there before (`reads_back`, and `own_reads_back` for its body alone), and those it writes after
it read what was there (`updates`): the picture stands a place on the left where the route finds
its content, on the right where the route leaves it, between two steps where one leaves it for
the next or fills it before it is read, and on both sides where the route reads it and writes on
it (`mathematics.md`, a route's domain and codomain). A body's choices carry what decides each
branch, as the code writes it, and a choice of one branch is a choice. The agent's route
text and the page are read from the same Flow, so the two see the same route.

**Repo** is a directory as read: the files its author wrote (no hidden directory, nothing git
ignores, no dependency or build directory) and the files named on the command line. **read: Disk
→ Repo** is the one place the engine touches a repository: the listing is read once, a file's
text once, when a reader first asks, and a part of the repository is a view of the same reading.
A package named on its own (`atlas-code view src/pkg`) is read as the project it is in, asked for that
package: finding the project is part of reading, and nothing after it looks at the disk. A part's
reading depends on nothing of the repository but what its reader asked of the `Repo`: each
question put to the listing (is this a file, which files of these kinds lie under that directory,
which directories in it hold this manifest) with its answer, and each file read with its bytes. A
worktree's `.atlas/index/` keeps each part's graph between readings with that witness and the
part's language environment as the conditions for reuse, so a file no question covers (a picture,
a note) leaves the kept graph in use. CLI, serve and MCP read through the same index; `--no-cache`
reads anew. A reading checks its complete witness before it is published, the parts found and the
environment with it. `atlas-code serve` draws a page once, at the libraries asked for, draws it
again only when its reading's witness no longer holds (or `.atlasignore` or the vision changed),
and keeps the last complete page available while the next is drawn. Its viewer loads
one HTML shell and receives each complete graph, flow and set of findings at `/api/snapshot`;
the selected route, filters and opened boxes belong to the view across those readings.
What a library's code and types are is the environment's, not the repository's: read by ty, from `node_modules`, from where Cargo keeps a crate. **Graph** is the one central type: typed nodes
(modules, classes, functions) and typed edges, and on each function the places in the
world its calls touch. Everything between parsing and drawing is `Graph → Graph`.
**Picture** is geometry: positions, shapes, labels. **Source** is a span of text.

**parse: Repo × Options → Graph.** Per language, per file; a repo is the union of its
files' parses, plus what the repo declares about itself (scripts, Makefile targets,
notebooks). The options (how far to read into libraries, the files named on the command
line) are an argument, never a setting of the process, so `parse` is a function: the same
files read with the same options give the same graph, to the order of its edges. *Decision: the engine may know languages and libraries, never a
project.* Knowing what a `requests.get` call means is the engine's job. What `rkg.sinks`
is follows from its code: the places its calls reach and the types it takes and gives.
Nothing about rkg, vectormesh or Atlas is hard-coded, and a project declares nothing to
Atlas, so that Atlas cannot flatter the repos it was built on. Where the picture differs
from the design the author has in mind, the code is what changes.

**view: Flow → Flow.** The whole middle of Atlas. Views compose, so a view spec is
a list of views, and the order is meaning: fiber and quotient do not commute in
general. A view only reads: it is the `get` of a lens with no `put`. Two families and
nothing else:

- **Fibers** keep the subgraph where a predicate holds: the pullback of `true` along
  the predicate, its nodes and the edges between them. Reachable from this entry point,
  hide private, hide tests, k-hop focus, only these edge kinds, only this commit range,
  only this part.
- **Quotients** identify nodes (the coequalizer of the classes): by base class, by package, by effect signature, by artifact. A quotient carries a
  refinement order, so "zoom in" is replacing a quotient by a finer one and "zoom
  out" by a coarser one. The quotients of a graph form a lattice; browsing is
  walking it.

**A view is data.** The code has hierarchies, each a forest the engine reads: parts into
packages into modules (`flow.units`), a route's steps into their bodies (`flow.bodies`),
families into their members (`flow.types`), a coarse place into the places it stands for, a
directory into its files. A zoom is an *order ideal* of such a forest: the set of nodes opened,
closed downwards (what is opened inside a closed box does not show). Its cut, the nodes
reached but not opened, is a partition of what the forest holds, so every ideal is a quotient,
zooming in adds one node to the ideal and zooming out takes a maximal one away. The ideals of a
forest form a distributive lattice (Birkhoff), and ideals of two forests (packages and families)
meet in their common refinement, so any combination of zooms is one picture. A zoom on types
identifies types, never moments: a value is versioned in call order (a step that hands on a `T`
makes a new one), and an opened family opens at each moment of its values, so the closed picture
is the quotient of the opened one, moment by moment. The moments are counted on the values, not
on the boxes the view draws: a step takes a value by the name that holds it, else the latest of
its kind (an override runs on the value its call runs on); two values with names of their own
are two values, also when they have one type (`Serial([a, b])` takes both).

A view is its fibers and its ideals: `{route, kind, part, focus, open: {parts, units, steps,
functions, families, places, dirs}}`. The page writes it to its address after every drawing, so
a picture is a link, and `atlas-code view --view` and the MCP tool `view` draw a page in a view an
agent wrote. A step opens by its path through the route (`pkg.cli.build/2`, the third call of
`build`) or, for whoever names functions rather than calls, every step of a function at once
(`open.functions`). The page applies the fibers first, then the quotients by the ideals' cuts.

A storage quotient groups only places in the current fiber. A symbolic disk place used both
ways stands as one place, rather than belonging to both directional groups. A single member stands as itself,
including its symbolic path when the concrete path is not known. Opening a group with several
members retains its name and frame around the members; the connections still end at the same
places. A closed step's reached effects cross its border and meet ports inside it. Its own
effects meet its border. Opening the step exposes the nested steps those ports stand for.

**When order matters.** For a quotient `q` by an equivalence `~` and a fiber by a predicate
`p`, filtering then grouping and grouping then filtering give the same picture exactly when `p`
is *saturated* by `~`: constant on every class, so that no class is cut in two. Keeping one part
is saturated by the package quotient (a package lies in one part), so its place in a view is
free. Hiding private names is not saturated by the module quotient (a module holds both), and
neither is a focus by the zoom that closes the focus's parent: there the order is meaning, and
Atlas takes the fiber first, so the reader decides what exists before the quotient decides how
it groups. One quotient is still chosen by the picture rather than the reader: files fold into
their directories when there are more than a column holds.

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
the picture; that is the zoom.

An opened route is the fiber of its calls, not of its functions. Each call is an arrow of
its own, in the order the body runs them: a function called twice is two arrows, and a
method called on a value of a subclass is that subclass's (the language's own dispatch, by the
receiver's type: ty's in Python, the declared and constructed types in Rust and TypeScript). A
method called through a class (`Cached.read(self)`, `Source.prototype.read.call(this)`) is that
class's, and a `super()` call the next one after the class it is written in, along the order of
the value's class: neither runs an override in its place. An arrow takes what its function declares and what its call
hands it, and hands on what it gives back; each value is the one the body holds by its
name, so `data` from one call is the `data` the next takes. A library's operation Atlas does
not read is an arrow where the body names its result (`tests = data["x"].to_numpy()`), or
where it changes the value it is called on (`rows.append(row)`: `list[Row] × Row → list[Row]`,
by the language's or the library's table of what changes its receiver, never by a name alone);
what it gives without a name goes into the call around it. Where ty does not know a value's
type, the value is still followed by its name, and no type is guessed. A body that gives
nothing back has only its own effects as its own arrow.

Three things make `F` honest, and each is an engine task:

1. Database reads and writes are different arrows. A graph database's direction is what
   the Python code says to its driver: a managed transaction (`execute_read`,
   `execute_write`), a routing or an access mode; what runs inside it goes the same way.
   A graph query's text is not read, and where the code says nothing the direction stays
   open. Python's own database interface (PEP 249) has only `execute(sql)`, so SQL is
   read, by its grammar and in its database's dialect: a statement writes the table it
   changes and reads the tables it takes from (`INSERT INTO a SELECT .. FROM b`), a
   setting names no table, and a table filled in at run time is not named. A statement
   is read where it is whole, evaluated as a path is: from a constant (`executescript(SCHEMA)`),
   an f-string or a template, the value a method of the program gives
   (`self._cte() + "SELECT .."`), or what the callers hand the parameter it is built from
   (`_scalar(con, sql)` reads the table its caller's `"SELECT count(*) FROM t"` names).
2. In-place mutation hides arrows: a function that takes a hub type and calls a
   mutator on it is an endomorphism `Graph → Graph`, not `Graph → ()`.
3. Disk is too coarse: `.env` and `assets/regelingen` are different resting places.
   Which argument of a call is a file is what its library's types say: a parameter typed
   as a path (`PathLike`), a file to read from (`SupportsRead`) or to write to
   (`SupportsWrite`) in any of its overloads, the path a method of a path is called on.
   The direction is what those types say or the call's own `mode`; a call that moves or
   copies reads the first path it is handed and writes the second, in every language
   (`shutil.move(src, dst)`, `fs::copy(a, b)`, `copyFile(a, b)`); else the operation's. A file
   handle is the file it was opened at. A file's place is its path evaluated from the code:
   literals and constants, `Path(..) / ..`, a property's or function's `return` (followed
   through ty's definitions), what a parameter is handed by its callers, and what an
   object holds: an attribute set in `__init__` from its parameters (`self.cache_dir =
   Path(cache_dir)`) holds what the constructor's callers hand it, or its default; so does a
   field of a class built from its fields (a dataclass, a model), TypeScript's `this.dir`
   set in the constructor or declared as a parameter property, and a Rust field as the
   struct literals that build its type set it (`Self { root }` in `new(root)`). That is carried
   out one call at a time, through each caller's own parameters in turn, so two parameters a
   call hands together stay together. A call of a method a family's member overrides is a call
   of that member, handing its parameters by place (`exporter.write(report, file)` reaches
   `CsvExporter.write`); made on a parameter, it runs the member the caller hands
   (`exportReport(new CsvExporter(), CSV_REPORT)`). Where a call leaves a parameter out it holds its
   declared default, and an entry nobody calls holds what it is run with (`@click.option(..,
   default=..)`). A reader evaluates an expression in its own syntax (Rust: literals,
   `const`s, `format!`, `Path::new`, `.join`, `.with_extension`, what a function of the
   program gives); carrying it to the callers and placing it is the same for every
   language. The user's home is `~`, the system's folder for temporary files `$TMPDIR`. A
   file named under a directory that is not known is that name under
   any (`artefacts/*/report.json`); a path whose last part is not known rests in the
   directory before it, so what one route unpacks into `D/{kind}/{id}` and what another
   lists in `D` meet at `D/`. The same place connects the route that writes it to the
   route that reads it.

   A path built from a function's parameters is a parameter of its arrow: `Repo::read(dir)`
   is `Disk(dir) → Repo`, and a route fixes it by composition. A path no literal makes is
   named by the value it holds where it is whole: the parameter of the function there
   (`analyze.dir`, and a method's with its class, `Repo.read.dir`), or of the class whose
   constructor takes it (`Store.path`). A helper's `path` and a method's `self.path` handed
   the same value are one place, and two parameters
   that only share a name are two. Each place keeps the
   function whose call made it whole, and a route shows it only when it reaches that
   function: `analyze` reads the folder its user names, and `changes` also reads the
   revision it writes under `$TMPDIR`. Where the code decides a place, the place also keeps the
   function whose own code writes what its path starts at (`Touch::fixed_by`), however many calls
   lie between: `default_rows` for the file `read_rows(path)` reads when it hands it
   `Path("data/rows.csv")`. A default a call leaves out is an input with a default, which decides
   nothing. An entry the program also calls is run by its users
   as well, with what they hand it, in its own route. What an object holds is whole where
   the object was built, and a path that takes both that and what the method's own callers
   hand is whole where both are. A route that reaches none of the callers that make a call
   whole (it was handed the object) sees the call as it was read. Outside a route (the
   principles, one function on its own) every place counts.

Where the composition happens outside the package (vectormesh wires `VectorCache →
Serial → Trainer` in scripts and notebooks), the resting places are types, the
arrows are constructor calls, and a notebook is a route read cell by cell.

### Why look at code

Four questions; each picture is a view plus a layout that answers one of them. A
picture that answers none is decoration.

| Question | View | Layout |
|---|---|---|
| **Orient** — what does this do? | the fiber of a route, quotient by effect signature | plaat |
| **Verify** — does it do what I designed? | the same views; the design is the author's, and where the picture differs from it the code changes | plaat |
| **Place** — where does new work land? | quotient by base class, k-hop focus | type lattice, the package overview |
| **Explain** — to a student or an agent | any of the above, saved as a view | the same, with notes |

### Atlas as its own subject

If Atlas obeys the algebra, its own story is `parse → view → layout` with nothing
else on the route, and Atlas can check that about itself. In Rust that maps
directly: an enum per node and edge kind, a `View` trait with composition, and a
check of the Atlas repo in its own tests. If the picture of Atlas in Atlas
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
| Engine | Rust: the `tree-sitter` crates, ty's crates for Python's types, `sqlparser`, `serde` | Grammars compile in; Python's types come from a type checker, not from guesses; quotients, fibers and effect signatures are small enough to own. |
| Viewer | one HTML file baked into the binary: SVG, d3 for pan and zoom, elkjs for layered layout, highlight.js for code | No server and no build step: `atlas-code view` writes a page and opens it. The flows are DAG-shaped, and layered layout is what a reader expects. |
| Agent | `atlas-code mcp`: JSON-RPC over stdio | The agent asks the same engine the page is drawn from. |
| Distribution | one static binary per platform, built by the release workflow | `cargo install --path cli`, or the install scripts. |

`atlas-code history` serves the viewer beside a repository's history, on 127.0.0.1 only, with the
standard library's TCP and nothing else: GET requests, one thread per connection, each revision
read once per commit and the working tree on every request. Live reload is planned (M2).

Repository layout:

```
atlas/
  engine/      Rust crate: parse · effects · view (fiber, quotient) · principles · changes · diff
  cli/         Rust binary: view · check · changes · history · mcp · analyze
  viewer/      the page the CLI writes, with its fonts
  docs/
```

## 3. The graph

**Nodes** — `module`, `class`, `function`, `method`, `external`; later `type_alias`, `constant`, `route`, `command`, `cell`. Fields: `id`
(stable), `kind`, `name`, `file`, `span`, `doc`, `params`, `returns`, `decorators`, `entry`,
`part`, `touches`, `sites`.

**Facts a reader records.** Each language says the same things in its own words; its reader
writes them down as facts, and nothing after the reader reads a language's names for them.
A signature is data: each parameter by name and type, with the one a method is called on
(Python's `self` or `cls`, Rust's `&self`) marked as the receiver, the one that takes the
remaining arguments as rest, and one whose type says the body may change it (Rust's `&mut`).
What it gives back is the type as written and the value it gives: out of the effect the
language wraps a value in (`Result<T, E>` may fail, `Promise<T>` arrives later) and out of a
reference; no value for the unit type (`None`, `()`, `void`); a value of the class it runs on
for `Self`. A constructor is a fact (`ctor`): calling it makes a value of its class. An
initializer fills in the value a call to the class makes (Python's `__init__`, a TypeScript
`constructor`): the class's own, else the first along its method resolution order; a function
on the class that gives one constructs it too (Rust's `new`, a Python classmethod `-> Self`).
Either way the arrow goes into `Memory(T)`. A private name
(`private`) is one only the code beside it may use: a Python name with a leading `_`, a Rust
item without `pub`, a TypeScript member marked `private`, `protected` or `#`, or a name its
module does not export. What a language leaves to its author is a fact on the part
(`leaves`): a package's exports, a signature's types, what a function gives back.

**Touches** — the calls of a body that leave the program, each typed by the part of the
world it reaches (`World`: disk, web, a database by its kind, a language model, a model,
a process, the screen, a figure) and the way the data goes (`Dir`: read or write) when the
call says. In the JSON a file's direction is part of its kind (`disk_read`, `disk_write`,
`disk` when the call does not say), another place's stands beside it (`dir`).

**Sites and what they hold** — the steps of a body, in the order they run: each `Site` is one
call, with what it is handed (`hands`) and what it gives back (`gives`). Each of those is a
`Held`: a value by its type and the name the code holds it by (`DataFrame · frame`), or, for a
value no name holds (one call's result handed straight to the next), by the step that makes it.
A route's lines between steps are drawn from these: a value one step gives and a later step is
handed is the line between them. Where a `Touch` is a call that leaves the program for a place,
a `Held` is a value that stays in it, passing from step to step in memory. On Atlas's own
picture, the readers fill a `Site`'s `hands` and `gives` as they read a body, and `graph_of`
turns them into lines.

**Edges** — `contains`, `imports`, `inherits`, `calls`, `instantiates`, `takes`,
`returns`, `has`, `mutates`, `variant`, `requests`; later `yields`, `raises`,
`decorates`.

**Entry points** — CLI commands and API routes (a function decorated by a command-line or a web
framework's decorator, by the library ty says the decorator comes from, whatever it is called: a
command by the name that framework gives it, a route by the method it serves and its path),
MCP tools (a function decorated by `tool`,
`resource` or `prompt` on a value ty types as a tool server, whatever the value is called), a router's
registration calls (`app.get("/x", handler)`, at the prefixes of `app.route("/api", sub)`;
an inline handler is a function named by its route), the programs and plugins `pyproject.toml`
declares (`[project.scripts]`, `[project.gui-scripts]`, `[project.entry-points]`,
`[tool.poetry.scripts]`), `package.json`
`bin` and the scripts that run a file of the package, Makefile targets (a recipe's line resolved to the program the project installs that it runs, a Python script, a crate's binary or an npm `bin`, and to the subcommand it hands that program when the program has one by that name),
the project's scripts and notebooks at any depth (every `.py` and `.ipynb` outside the package,
its tests, what git ignores and another package or project inside it, and the files a declaration
names, which are modules of the program), and what
a package exports (`__all__` or the public names its root `__init__.py` binds, and the
`__all__` of each public subpackage, as `components.Serial`; the `pub` items of a library
crate's root and of the modules it makes public with `pub mod`, as `view::hide_private`, and
the names a public module re-exports with `pub use`; the names exported by the module a
`package.json` points to with `exports`, `module` or `main`). A re-exported name is followed to
where it is defined. Each is the
start of a route; an exported class is a route whose steps are its public methods. A program a
manifest declares is a command also where its package exports the function (a flat module's
public `main`): the declaration says what the function is to its user, and the function is a door
as well (`Node::exported`). A declaration `name = "mod:attr"` names `attr` in the module Python
imports as `mod`, whatever Atlas calls that module: a flat layout's `ask.py` beside the package is
`ask`, as a src layout's `src/ask.py` is. The file it names is entered through the declaration,
not also as a script of its own, and where it runs as a program it is entered as a package's
module is, by `python -m`. A file a `bin` names, or one run as `__main__`, is the program's start:
its top level is its route, and a call of `main` there is a step of it. An entry is an
arrow whose domain is what the human hands it on the command line: a command or script carries
its options (`Entry::options`, `--raw` named, `<name>` positional) and reads as its usage line,
and the call that parses the command line (`Site::command_line`) is the entry's, not a step of
the route, so the steps that use an option are fed by the entry.

**Parts** — a repository is one or more parts: what one manifest roots (`Cargo.toml`,
`pyproject.toml`, `package.json`), at any depth, read in its own language. A manifest that
a part around it reads is that part's: a Cargo workspace reads its member crates (a crate
beside them that is no member, `fuzz/`, is a part of its own), a Python project its
packages (each member of a uv workspace is a part), a package the code under it. Nothing
under `tests/` or `node_modules/` is a part, nor a manifest without code of its own (a
workspace's root). A Python project's packages are the ones its `pyproject.toml` names (its
build backend's `packages`, else the project's name, found as Python imports it: a directory
with `__init__.py`, a single `name.py`, or a namespace package), then every other package
beside them; the graph is named after the first, each package is a part, and every package is
the project's own (its imports, its exports). A crate is read
by Cargo's targets: one library (`src/lib.rs`) and a binary per `main` (`src/main.rs`,
`src/bin/*.rs`), each a root of its own; a directory with its own `Cargo.toml` is another
package, named by its `[package] name`. A TypeScript package is named by its `package.json`'s
`name`, which code imports it by (`@acme/client` as it is written), and by its directory only where
that names none. The graph holds them side by side and every node knows its part. Two parts that hold a
top-level name alike (two projects whose package is `app`, the scripts beside each) hold it under
the directory each part lies in, `orders/app` and `billing/app`: the code does not tell them apart,
so the picture names them by where they lie. A name one part holds keeps it, and so does the
project at the repository's root. A Makefile is its part's, a unit beside its packages: two
Makefiles are `orders/Makefile` and `billing/Makefile`, with a target both name (`run`) held the same
way, and the Makefile of a directory two parts are rooted in (a `pyproject.toml` beside a
`package.json`, each part read in its manifest's language) is one. Parts meet over HTTP: a web framework's
routes stand at their full path (through `app.mount(prefix, sub_app)`,
`include_router(router, prefix=…)`, `APIRouter(prefix=…)`), and a request one part makes
whole that none of its own routes serves is a `requests` edge from the function that makes it
whole to the handler of another part's route that serves it, by method and path. Only a request
to the machine the parts run on (`localhost`) or to a host the code does not say (a value the URL
starts with) may meet another part; a host the code writes out is another machine's. A request whose path the code does
not say (a key looked up at run time, an environment variable that may stand in for it) asks
for no route: it stays on the picture, sent to an address the code does not name. In the views, parts are a zoom level like families: the overview
is one step per part (memory stays inside it; what it reads and writes in the world, and
the HTTP of the parts it requests, stand outside), a click opens a part into its packages,
and keeping to one part is its fiber, in which another part is the web place it is
reached through. `atlas-code check` reads each part in its own language.

**Packages** — one level down, the same quotient by module path: a part's packages (the
children of its own package, a crate's modules, an app's directories; scripts and
notebooks beside them as one each), a package's subpackages, and at the bottom a module's
routes and doors (the functions other packages call in it). A package closed is one step:
the places its own code touches, not what it reaches through other packages, and the
types that cross its edge. A type crosses where one package makes it (gives it back,
builds it, changes it) and another takes it, and between packages it is drawn once, as
the joint. What crosses is data: a class one of whose operations reaches a database,
the web, a model or a program (a sink, a repository, a client), and a library's type
whose calls reach one (`sqlite3.Connection`), is a door to that place, and the place is
drawn instead. A file or the screen makes no door: a value may save or show itself.
Packages whose code calls one another get a dotted line, from the caller: the quotient
of the call graph, beside the quotient of the places, drawn whatever data also joins them. The package
level stands whether or not entry points are found.

**No declared architecture.** A repository declares nothing to Atlas: no roles, no
order, no layers. What a package is follows from its code, and the layers between
packages are the order their dependencies make. Packages and the imports and calls
between them form a preorder; where two packages depend on each other there is no such
order, and that cycle is what the code shows.

## 4. Analytics, as views

Each layer is a pure function of the layer below and can be tested alone.

**L0 Syntax (tree-sitter).** Per file, per language, `.scm` queries yield
definitions, signatures, imports, calls, annotations, decorators, docstrings,
string literals at boundary calls. A new language is new queries, not new code.

**L1 Resolution.** Names become node ids: imports (absolute, relative, re-exports),
bases, and calls through the type of their receiver.

Python's types are ty's, Astral's type checker, which reads the project with its own
environment: the one `[tool.ty.environment]` names, else the project's `.venv`, never what is
active in the shell (*Decision, PTT-487*: the same files give the same graph wherever Atlas runs).
For each call ty says where it goes: a definition of the package, matched to its node
by where its name is written, or a library's module. For each value it says what type
it has, and which classes that type names (`list[Node]` names `Node`). The two trees,
tree-sitter's and ty's, meet at an expression's byte range; a notebook's cells are
joined as ruff joins them, so the positions agree.

A library the project's code calls is read as code where its source can be followed: the
modules the project's calls go into, and with `--libraries N` the modules of the same library
those call, N rounds deep (one by default). What the imports hold is then within reach, and
the libraries behind them are not followed to the bottom. Its functions are steps like the
project's, and what it reaches of the world is the world the project reaches through it
(goad's `FileHandler.download` fetches a URL and writes the cache). What it holds but the
project does not reach is dropped; what it calls in another library stays that library's call.
Not read: the standard library, a package the effect tables already describe (a
network, database, file or model library, plotly), and a package with compiled code, whose
source cannot be followed to the bottom. A library's code is drawn where a route steps into it,
not as a package of the project, and `atlas-code check` does not judge it. A class handed to code
Atlas reads (`pipeline.add(DiffValues)`) is built there.

The rule is the same in every language. TypeScript reads a package in `node_modules` from what
its `package.json` exports as code, through its re-exports; Rust reads a crate where Cargo keeps
it, a path dependency or the version the lock names, through its `pub use`. What the graph keeps
of a library is the fiber over "reached from the project" (`view::reached`): the functions the
project's calls reach within the N rounds, a round a call, and the types those take, give and
run on; the rest of a module read is dropped, so the graph grows with what the project uses, not
with the size of the library (*Decision, PTT-491*). What lies beyond the fiber is not lost but
glued into the library function at its border: every body of the code read is traced, and a
kept library function is one arrow that touches every place its code reaches inside the library,
as an operation the tables describe does; the project's own functions keep their own arrows apart
from what they reach. Of a library's file the graph keeps only the lines its kept nodes show.

What Atlas knows of libraries and frameworks is data: `engine/src/knowledge.rs`, one table per
kind of fact. Which package reaches which part of the world (by registry: PyPI, crates.io,
npm), a library's operations that cross by themselves, the HTTP methods and how a framework
mounts a router, the parameter a web call names what it asks for by (`url`, `repo_id`), which
methods change or hand on what they are called on, a path library's functions. A reader looks
a fact up; no reader tests a library's name in its control flow, and a test keeps it so.

Where ty does not know a type, Atlas does not guess. A function without a return
annotation gives back `Unknown`, a call on that value reaches nothing, and `atlas
check` asks for the annotation. Without an environment, a dotted name through an import
is still what the import binds (`requests.get` is `get` of `requests`); what the call
gives back has no type.

Rust's and TypeScript's signatures carry their types, and a receiver's type is
composed from them. In Rust, `Self` is the impl's type, a trait's provided method is reached
through a type that implements the trait, a method neither defines (`to_string`, `clone`)
reaches nothing, an `enum` is a sum of the types
its variants carry, and `matches!(k, "a" | "b")` compares like `match`. A function in an
`impl` without `self`, a Python class or static method and a TypeScript `static` method are
called on the class, so the class is not their input.

In every language, a function of the package handed on as a value (an executor's
work, a thread's target, a callback, an iterator's `map`) is called by whoever
receives it, so it is a call from where it is handed. Reading a property hands
nothing on. Output: the symbol graph plus a resolution report (resolved,
external, builtin, local, unknown) so the quality has a number.

**L2 Effects.** Boundary tags on every call that leaves the package (web, disk in
and out, each database read or written, LLM, model, process), by the library that
defines what is called and by what that library's API does: a network library's call
that is handed a URL (or sends on a socket) moves data, and a statement, a connection
or a commit reaches a database, while building a client (a web or LLM library's class)
or reading a response or rows that have arrived moves nothing. A database is named by
what its driver reaches: the Bolt driver (`neo4j`) a graph database, whichever one. A figure
is drawn to be seen: `show()` puts it on the screen, and a notebook shows each figure its cells
make (matplotlib's `figure`, `subplots`), so on a notebook's route a figure is on the screen. Rust
has no type checker to ask, so a library's value is traced to the call of a function on its
type that made it, through the locals that hold it, or is of the library's type its parameter or
its field declares (`conn: &Connection`, `self.conn`), made wherever it was: building (`new`, `builder()` up to
`build()`, `default`) moves nothing, and the value moves data where it is used
(`client.get(url).send()`, `OpenOptions::new().append(true).open(p)` writes). A value whose
making touched a place (`File::open(p)`, `Command::new("git")`) is at that place; a call on it
adds one only when it names its own (`conn.execute("INSERT INTO log ..")`). In TypeScript a
client is what a package's constructor or `create…` factory made (`new Pool()`,
`new Anthropic()`, `axios.create()`), or what a parameter or a field of the class is declared as
or set from (`private db: Database`, `this.db = new Database(file)`): a call on it is the package's, and a language model is
asked only through one. The browser's `localStorage` is a key-value store, keyed by the literal
it is handed; a URL without a host is the page's own origin. A TypeScript path or URL is
evaluated as a Python one: from the module's own file (`import.meta.url`, `__dirname`),
through `path.join`, `path.resolve`, `path.dirname` and `new URL(rel, base)`, across the
constants it names, through what a function of the program gives back (its parameters bound to
what the call hands them), and at each call that hands its function's parameters, out to the
function where it is complete. A request is its method and its URL, in every language: the URL
waits for the callers that make it whole as a path does, and the method is the operation's
(`get`, `post`) or what the call is handed (`fetch(url, { method })`). A page's request without a
host goes to the page's own origin, so one the package's own routes serve is a `requests` edge
from the function that names it to the route's handler, the most literal route first (`/years`
before `/{year}`); a client that is no page (Python's, Rust's) sends such a URL where it
connects, which the code says apart from the request. An object of functions
(`const api = { get: (path) => .. }`) is a namespace; one with a declared type is a value of
that type. Artifacts by path
pattern, hub types by arrow count, mutation detection, and the effect signature
`F: Graph → W` per function and per route. This is the functor behind the plaat.

**L3 Quotients.** By base class, by package, by part, by effect signature, by artifact;
each with its refinement order. The quotient graph aggregates edges per class pair and
kind.

**L4 Fibers and structure.** Reach from an entry point (dominator tree for
"what does this command touch"), k-hop focus, edge-kind filters, commit ranges; SCC
condensation for import cycles; betweenness for hubs; k-shortest paths between two
types for the signature flow; Leiden as the honest fallback when nothing is
declared.

**L5 Git.** Two revisions are two readings, A and B, each read from git into a directory of its
own. The picture of a change is their union A ∪ B, marked as a diff marks lines: what B adds is
added (green, `+`), what B no longer has is removed (red, dashed and struck through, `−`), and what
both hold is changed (amber, `~`) when its own arrows differ. The layout is the union's, so a
switch between A, A ∪ B and B hides boxes and never moves one.

A shared call occurrence has the same target, receiver, typed inputs and output bindings, in
call order. Which call made an input is no part of it: that is the arrow of the call that made
it, so `raw = load(p)` becoming `raw = fetch(p)` marks that occurrence, and `smooth(raw)` after
it stays the same. Anonymous output names belong to their reading; a shared occurrence aliases
A's anonymous values, those it is handed and those it gives, to B's names, so each side retains
its own value chain. A call put in before a shared one (`double(x)` becoming `double(add_one(x))`),
or taken out, is the only occurrence marked: the shared one is handed in each reading what that
reading hands it, and the union keeps what A hands it beside it (`Diff::hands`). A value a shared
occurrence is handed under one name in both readings is one value, also when another call makes
it in B (`Diff::remade`): `Frame · raw` is drawn once, with a removed line in from `load` and an
added one from `fetch`, and is changed, since the arrows into it differ. Calls to one function
with different inputs remain distinct occurrences without changing the function's own arrow.

*Decision (PTT-509): changed is Yoneda, applied to the generators.* An object is determined up to
isomorphism by the arrows into and out of it. A place or a type is changed when the arrows directly
into or out of it differ: the same file, written by `plot` in A and by `Trend.save` in B, is
changed, and keeps its name. A function is an arrow, so its identity is its own arrow: what it
takes and gives, what its own calls read and write, and for an entry its usage line. A step its body
gains or loses is marked itself, as added or removed. Only direct arrows count (sentence 6): a
change upstream marks the step that changed, never what runs before or after it, and a place a
caller makes whole is the caller's arrow. A file named by a function's own parameter (`load.path`,
`Store.path` for what a constructor takes, `Repo.read.dir` for what a method takes) waits for what the
function is handed: it is named so whoever calls the function, and is no arrow of the function that
hands the parameter on.

*Renames.* A node B no longer has and a node B adds whose direct relations are isomorphic (the same
typed arrows in and out, the node itself and what is named under it counting as the same) are one
node, renamed: it is changed, and reads `old → new`. A function's typed arrow is among its
relations (`list[float] → float` is no `str → str`, and a parameter more is another arrow), and a
place named by its value (`plot.out`, `Repo.read.dir`) is named under it. A node named under a
renamed one (a method of a renamed class) keeps its own name, and is changed only when its own arrow
is. Only a match that is the only one either way counts; two that could match either way are removed
and added, since Atlas never guesses.

The engine marks the nodes, the edges, the steps and touches of a node both readings hold (by
index), and the places, by the calls that touch them. In a picture a value stands at its moments,
so a box of a value is changed when a line drawn into or out of it is in one reading only: the page
draws each reading by itself and marks what their union holds once. The page shows no list of
changes; `atlas-code changes A..B` and the MCP tool `changes` give the agent the list, read from the
same union: each route in A and in B, the steps and places it gains and loses, those it has in both
that the union marks changed, and apart what the union marks that no route reaches. `atlas-code history`
serves the commits and these pictures on this machine.

*Decision (PTT-495): a closed box is a point of a quotient.* Its own arrows are the lines that cross
its border, so its own mark comes from them alone: a part, a package, a family or a place that stands
for others is changed when a line drawn into or out of it is in one reading only, and a closed step
when its own arrow differs. What happens inside it without touching the border is reach, not the box
(sentence 6): a dashed tag at its corner counts it, `+3 −1 ~2` for the diff and the number of findings
for the principles, each marked node, step or place once, at the deepest level the picture draws it,
and opening the box shows where they are.

**L6 Agent.** An MCP server over the same engine: `search`, `neighbourhood`,
`effects`, `quotient`, `signature_paths`, `apply_view`.
Views are JSON specs, a list of fibers and quotients plus a layout, that
the agent writes and the browser renders. Annotations are agent explanations stored
per symbol.

## 5. Visualisation

Every view answers one of the four questions. If a legend needs more than three
items, it is two views.

| View | Question | Picture |
|---|---|---|
| **Flowchart** | Orient, Verify | The image of the effect functor: resting places as boxes, quotiented arrows between them, named by base class or package, and by signature otherwise. Click a box to refine it one level. |
| **Type lattice** | Place | Inheritance forest; external bases as ghosts; wide leaf families as a grid; badges for abstract methods and overrides. |
| **Signature flow** | Orient | Types as nodes, functions as arrows, ELK-layered; select two types and the composition paths light up. |
| **Git graph** | Verify | Commit DAG; click a commit or drag a range → overlay on any view. |
| **Inspector** | all | Read-only code with room for 100 columns, signature, docstring, in/out edges, annotation, open in editor. |

**Design language.** Package = hue: the first level of the code quotiented by module
path, the level a part's overview shows (one muted palette, the same in every view). Kind =
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
  vectormesh second and third. Friction goes straight to an issue.
- **Design rounds** are explicit issues with the label `design`: three screenshots,
  one question, one decision.
- **Decisions** are recorded on the issue that made them, with the reason.

## 7. Open questions

1. Files fold into directories by how many a column holds: a default the layout chooses.
   Which default ideal of the file tree follows from the code instead?
2. Is the package the right unit for colour, or is the base type?
3. Editing: probably never. "Open in editor" as a deep link should be enough.
4. The browser as a resting place: a click in a frontend is an arrow into an API route,
   where the request names the path it asks for.
