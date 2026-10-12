# Atlas — principles

Atlas draws code as places where data rests and the functions between them. When the
picture looks wrong, the cause is usually a design principle the code does not follow
yet. `atlas check` computes those principles from the same graph the picture comes
from, and reports where code and principle disagree.

```bash
atlas check            # the repository you are in
atlas check ../goad    # another one
atlas check --json     # for an agent or a script
```

A coding agent asks the same through MCP. `atlas mcp` serves seven tools over stdio:
`principles`, `check`, `explain` (a principle's section of this page), `routes`, `route`
(what one entry point does, step by step), `view` (the picture opened where the agent points,
written as a page for the human) and `changes` (what an edit did to the routes). In Claude Code:

```bash
claude mcp add atlas -- atlas mcp
```

Any other agent takes the same command: `{"command": "atlas", "args": ["mcp"]}`.

Each finding names the principle, the place in the code, the evidence the graph holds,
and ends in a question. Atlas coaches; it does not fix. The author decides what the
answer is, changes the code, and the picture follows.

`atlas view` and `atlas check` read the same graph. The view is the map: your eye finds
the box that should not be there. Check is the list of places on that map that deserve
a second look, found by the engine, which sees every type, call and effect at once.

Atlas is not a linter. ruff, pyright and clippy own the line level: style, an unused
import, a missing annotation as such. Atlas owns the level of types, routes and places:
which doors a package has, what travels between modules, where data rests. It asks for an
annotation only where a route stops without one.
The two work together. A linter makes a signature precise; Atlas asks whether it is
the right one.

Atlas follows these principles itself: `atlas check` on Atlas's own repository finds
nothing, and `cargo test` keeps it that way.

## A package names its doors, and each door says what goes in and what comes out.

Id: `doors`.

A user meets a package at its doors: the names it exports at its root. Each door is a
route Atlas can draw, from what goes in to what comes out. A package that names no
doors, or a door that does not say what it takes and gives, leaves a reader (and
Atlas) guessing where a route goes.

- **Python.** Declare the public names in the root `__init__.py` with `__all__`
  ([PEP 8](https://peps.python.org/pep-0008/#public-and-internal-interfaces)), and
  annotate every parameter and return of a public function
  ([PEP 484](https://peps.python.org/pep-0484/)). To keep `import package` fast, the
  exports can resolve lazily with a module `__getattr__`
  ([PEP 562](https://peps.python.org/pep-0562/)).
- **TypeScript.** What the module `package.json` points to exports is the package's doors.
  Annotate what each one gives (`isolatedDeclarations` asks for it); a function handed to a
  call where it is declared (`memo((props) => ..)`) takes its parameters' types from the call.
- **Rust.** `pub` items at a library crate's root, in the modules it makes public with
  `pub mod`, and what it re-exports with `pub use` are its doors, and the compiler asks
  for every type. Atlas has nothing to ask here.

Atlas checks: a Python package called `src`, a Python package that exports nothing at
its root, and an exported function or a public method of an exported class whose
parameters or return are not annotated.

A package needs a name before it can have doors. `src/` is the folder packages live in
([the src layout](https://packaging.python.org/en/latest/discussions/src-layout-vs-flat-layout/)):
code in `src/__init__.py` installs as a package called `src`, the same name as every other
project laid out that way. Under `src/<name>/`, `uv sync` installs it as `<name>`, and
`from <name> import …` works the same in a script, a test and a notebook.

A package whose root binds no public name has no route in as a library. goad names
four:

```python
__all__ = ["DataProcessor", "Pipeline", "BasePlot", "DistributionFitter"]
```

and each of them is a route in the picture.

## A sum type is unwrapped where it arises.

Id: `sums`.

A sum type says a value is one of several: `Result = FitResult | FailedFit`. Someone
has to choose which one it is. When the sum travels on unopened into other modules,
every consumer makes that choice again, and each of them has to know about every
variant. Unwrapped where it arises, the choice is made once, and what travels on is
the variant each consumer actually uses.

- **Python.** A sum is a union of types: `A | B`
  ([PEP 604](https://peps.python.org/pep-0604/)), with dataclasses as the variants
  ([PEP 557](https://peps.python.org/pep-0557/)) and a name for the whole
  (`type Result = A | B`, [PEP 695](https://peps.python.org/pep-0695/)). It is opened
  with `match` ([PEP 634](https://peps.python.org/pep-0634/), tutorial in
  [PEP 636](https://peps.python.org/pep-0636/)), and `typing.assert_never` makes the
  type checker confirm every variant is handled. Errors that stop the program stay
  exceptions; a sum is for an outcome you want to keep or report.
- **Rust.** `enum` and `Result<T, E>`, opened with `match` or `?` at the call site. A
  function that takes a `Result` as an argument has received a choice its caller
  should have made.

Atlas sees a named union of two or more types as a type of its own, with an edge to
each variant. It checks: a sum that one module returns and that public functions in
other modules take.

goad: `DistributionFitter.fit` returns `list[Result]`, and both `PlotFits.plot` and
the CLI's `format_results` take it and filter out the failures themselves. A plot has
no use for a failed fit. One way to open the sum where it arises is to let the fitter
hand back both halves, each with its own type:

```python
@dataclass
class Fits:
    fits: list[FitResult]
    failures: list[FailedFit]
```

`PlotFits.plot` then takes `list[FitResult]`, and a report takes both.

## An override keeps the promise of its base.

Id: `overrides`.

A caller written against a base class must be able to use any subclass in its place
(the Liskov substitution principle). An override that hands back something its base
does not promise breaks that: the caller expected a `Tuple[Figure, Axes]` and got a
`Figure`, or `None`. Handing back a subclass of the promised type is fine.

- **Python.** Type checkers report it as an incompatible override; pyright's
  `reportIncompatibleMethodOverride`, ty's `invalid-method-override`. A
  `# type: ignore[override]` on the line is the code saying it knows.
- **Rust.** A trait implementation must match the trait's signature; the compiler
  checks it.
- **TypeScript.** A method must be assignable to the one it overrides; the compiler checks
  it.

Atlas checks: an override whose return type cannot stand where the one its nearest base
declares stood, grouped per base method. It decides by what the types mean, not by how they
are written. For Python, ty decides whether the type is assignable: `List[Node]` is
`list[Node]`, `Optional[Node]` is `Node | None`, `Annotated[str, "a note"]` is a `str`, and a
`list[Node]` may stand where a `Sequence[Node]` is promised. Where ty does not know a type
(a library the environment does not hold), it has nothing against it, and neither has Atlas.
For Rust and TypeScript, whose compilers already hold an override to its base, Atlas compares
the program's types the two signatures name: each type the override gives is one its base
promises, a subclass of one, or a variant of a sum it promises. `anyhow::Result<Page>` keeps
the `Result<Page>` a trait promises. An array's shape is a promise no type checker reads
(jaxtyping's `Float[Tensor, "batch dim"]`): each array an override gives has a shape its
base allows, where `...` stands for any run of dimensions, a name for any one, and a number
for itself.

goad: `BasePlot.plot` promises `Tuple[Figure, Axes]`; `DecomposePlot.plot` and
`PlotFits.plot` give a `Figure`, and their `build` gives `None`. The question is
whether they are kinds of `BasePlot`, or plots that use one: composition, where a class
holds a `BasePlot` instead of extending it, keeps both promises honest.

## Library code does not decide where data lives.

Id: `places`.

Where data rests is decided where the program starts: a script, a CLI, a test, a
notebook. Code that those call takes the place as an argument. A library that names
its own file or URL works in one directory on one machine, and a test cannot point it
elsewhere.

- **Python.** Take a `Path` (or a `str | Path`) parameter; let the script build it
  (`Path("data") / "raw.csv"`) and hand it in.
- **Rust.** Take `impl AsRef<Path>`; let `main` decide.

Atlas checks: a read or a write in library code whose path or URL starts at a literal
(`open("out.csv")`, `Path("data") / name`, `f"out/{x}.png"`). A path that starts at a
value from elsewhere (`root / "out.csv"`, `root.join("Cargo.toml")`) is decided by the
caller. Where the program starts may name places: an entry itself (a CLI command, a route, an
MCP tool, a test, a binary's `main`), and a file that runs as a whole (a script, a notebook, a
test file) with the code it defines. A function beside an entry is library code all the same:
a helper in the module of a CLI command, or a function in a Rust file whose `#[test]`s sit
below it, takes its place from whoever calls it.

goad: `BasePlot.save(path)` takes the place, and the demo script, where the program
starts, names it:

```python
plot.save("data/result/zscores.png")
```

## A change to an input shows in the signature.

Id: `changes`.

A function that changes an object it was handed has an effect its caller cannot see.
The caller holds the same object afterwards, and it is different. That surprise belongs
in the signature, or not in the function.

- **Rust.** `&mut T` says it, and the compiler holds everyone to it. Atlas has nothing
  to ask.
- **Python.** No signature can say it. So either the object changes itself, through a
  method of its own (`graph.add(node)`), or the function leaves its input alone and
  hands back a new value (`sorted(xs)` next to `xs.sort()`). A method that changes
  `self` is what methods are for.
- **TypeScript.** The same as Python: a `Readonly<T>` parameter can promise no change, but
  nothing promises one. A method that changes `this` is what methods are for; a function
  that fills an array it is handed (`lines.push(..)`) can hand back the array instead.

Atlas checks: a Python or TypeScript function that changes a parameter other than its
receiver, found from assignments to its members and items and from calls that change it
(`append`, `push`, `set`).

goad: `BasePlot.plot_on(other_plot)` sets `other_plot.fig` and `other_plot.ax` from the
outside, then asks it to plot. The question is whether `other_plot` could be told where
to draw by a method of its own.

## A function whose value is used says what it gives back.

Id: `gives`.

What a function gives back travels on: its caller stores it, hands it to the next function,
calls a method on it. A return annotation is what lets a reader follow it, and a type
checker check it. Without one, the value has no type at the caller, and the route stops
there.

- **Rust.** Every function says what it returns. Atlas has nothing to ask.
- **Python.** The annotation is the author's to write. Atlas reads Python's types through
  ty, Astral's type checker, and ty gives a function without a return annotation the type
  `Unknown`. Atlas does not guess behind it: a database call on that value is not drawn,
  because nothing says which database it is.

Atlas checks: a Python function or method without a return annotation, whose value at
least one caller uses. A call that stands as a statement does not use it. A door is asked
about by `doors`. The finding names three of the callers that use the value and counts the
rest.

rkg: `RegisterSink.transaction(self)` returns a connection to the register's SQLite
database. Every `with sink.transaction() as con: con.execute(..)` then runs on a value
without a type, and the register's tables drop out of the picture. With
`-> AbstractContextManager[sqlite3.Connection]` they come back.

Atlas sees best with the annotations the linters ask for. Ruff's `ANN` rules ask for every
parameter and every return:

```toml
[tool.ruff.lint]
extend-select = ["ANN"]
```

## A choice is made once, in a place with a name.

Id: `choices`.

Code chooses: which experiment, which criterion, where a value comes from. A choice that
is made in one place, under a name, is one thing to read and one thing to change. A
choice spread over a body is neither.

Two shapes of the same problem:

- **A value handed in is tested in several places.** `mode == "parallel"` here,
  `mode in {"weighted", "parallel"}` there: what `"parallel"` means is spelled out again
  in each place, and a new mode is a change in all of them (Fowler's *repeated switches*).
  Made once, the choice becomes a table or a type that says what each case stands for.
- **Alternatives sit between other steps.** `load(p)` or `find(g)` or `infer(g)`: three
  ways to get one value, in the middle of a body that also does other things. Where the
  value comes from is a step of its own; given a name, the body reads as its steps
  again.

- **Python.** A `Literal["a", "b"]` or an `Enum` for the cases, and one dict from case
  to what it means: `EXPERIMENTS: dict[str, Experiment]`, with `Experiment` a dataclass
  ([PEP 557](https://peps.python.org/pep-0557/)) holding the facts each `if` asked
  about. A choice between steps becomes a function whose body is that choice, written
  with `match` ([PEP 634](https://peps.python.org/pep-0634/)) or `if`/`elif`.
- **Rust.** An `enum` for the cases with one `match`, and methods on the enum for what
  each case means. A choice between steps becomes a function whose body is the `match`
  (or the `unwrap_or_else` chain).

Atlas records, for every call, the arms of the choices it sits in (`if`/`elif`/`else`,
`match`, `try`/`except`, `a if c else b`, `x or y`; Rust's `if`, `match` and
`unwrap_or_else`/`or_else`), the name the body keeps its value by, through the arms of a
choice (`g = a() if c else b()`), and for every comparison of a value with string literals,
the choice it decides. It checks: a parameter (or `self`) tested against the same literal
in two or more places, and alternatives in two or more arms that each make a value of one
type for one later step of the body, by the name it is kept by or handed straight on. It
reads every call as its own step, so the same constructor in two arms is two alternatives.
A choice whose value the body returns, or whose arms each finish their own case, is the
body's name, not a step without one.

A student's exam project: `run_standard_experiment(args)` tests `args.experiment` in four
places, and `"parallel"` comes back three times. One `Experiment` per name, holding
`use_2d`, `weighted`, `parallel` and the model to build, makes it one place.

In Atlas, `file_direction` is such a step: which way a file argument goes (`Reads |
Writes | src | dst | what the operation does`) is its whole body. Written inline in
`file_calls`, the five arms would sit between reading the call's literals and placing its
file, and `atlas check` asks for their name. Named, the body reads as what it does:

```rust
let f = effects::facts(tag, lib, op, &lits);
let kind = file_direction(file, name, paths, &f);
```

## A call leaves something behind.

Id: `nothing`.

A function is an arrow from what it takes to what it gives, writes or changes. One that gives
nothing back, cannot fail, changes nothing it is handed and reaches no place is an arrow into
nothing: whoever calls it learns nothing and nothing is different afterwards. Either it is
dead, or it changes something the code does not show: a value it was handed, reached through
an item or a loop; a global; a library's own state. Then the change belongs where a reader
sees it.

- **Rust.** `&mut T` and `-> Result<()>` say what a function changes and that it may fail.
  What stays hidden is a `static` or a value behind a `RefCell`.
- **Python.** A function that fills a value it is handed (`for node in graph.nodes:
  node.status = ...`) can be a method of that value, or hand back what it makes. A function
  that only writes a global (`SEEN[name] = name`) can hold that state in a value of its own.
- **TypeScript.** The same as Python; a function that sets fields on the elements of an
  array it is handed can hand back the array.

Atlas checks: a function another function of the project calls, that gives nothing back
(Python `-> None`, TypeScript `void`, Rust no return type), whose body does not raise, throw
or panic and whose return type does not say it may fail (`Result`, `Promise`), and where
nothing it runs (what it calls, what it builds, the overrides a call may reach, down to the
end) touches a place, changes a value or a parameter, fails, or calls a function it was handed.

rkg: `stempel_herkomst(graph, herkomst) -> None` sets the author, status and date on every
node of the graph it is handed, through a loop. Atlas sees a function into nothing; what it
does is change the Graph. As a method of the graph, the change shows as what it is.

## The packages of a part depend on one another in one direction.

Id: `layers`.

The code of a part, quotiented by package, is a preorder: a package depends on another when its
code calls, builds, imports or extends what the other holds. Where that preorder is a partial
order, the packages have layers: each can be read, tested and replaced with only what lies below
it. A cycle has no below. The packages in it are one package in all but name, and the link that
closes it is usually one call against the flow.

- **Python.** A package that imports from the package that imports it, often inside a function
  to get past the import cycle, is the sign. What both need moves down into a module of its own.
- **Rust.** A crate's modules may use one another freely; the compiler does not stop a cycle.
  What belongs to the lower module moves there, and the higher one calls it.
- **TypeScript.** An interface module that also picks the implementation (`kiesBron` beside
  `BronLike`) imports the implementations that import it. The choice moves up, to whoever uses
  the interface.

Atlas checks: at every level of the package forest, the packages under one parent, from the
calls, constructions, imports and base classes of their code (a test is a way in from outside,
not a layer). It names each cycle with its links, the thinnest first.

In Atlas, `model` serialized a touch with where the picture draws it, calling up into `flow`,
and `paths` asked `parts` whether a route serves a path. The first now stands in the flow
(`Flow::drawn`), the second in `paths`, and the engine's modules are layered.

## What a lens puts, it gets back.

Id: `lens`.

A class that writes a place where data rests and reads it again is a lens on that place. Its
puts take a value and change the place, `A × S → S`; its gets give a value from it, `S → A`. Put
a value, get it again, and it is the value you put: `get(put(a, s)) = a`. When that holds, the
place has a type, and a route through it composes: a `Graph` goes in and a `Graph` comes out.
When a put takes a `Graph` and the gets give back a `dict`, or a list of rows, or nothing, the
place holds something no type names, and the flow through it falls apart into pieces.

Where data comes in from the world it comes in as the world has it (a CSV from the web, a JSON
payload); a lens is where the code turns that into its own types, once.

- **Python.** A store that saves a model (`noteer_verbruik(v: Verbruik)`) reads it back as
  that model (`lees_verbruik() -> list[Verbruik]`), not as `list[dict]`.
- **Rust.** `serde` gives both directions from one type: `save(&T)` and `load() -> T`.
- **TypeScript.** The same as Python: what a store puts as a `Regel` it gets back as a `Regel`.

Atlas checks: a class whose public operations both write and read one place (a database as a
whole, a file by itself; through what they call), for each type of data a put takes that no
get of the class gives back from that place. A class that only reads a place (what comes in
from the world) or only writes it (an end product) is no lens on it. Where another class gets
that type from the same place, Atlas says which: the lens is then split over two classes. A put
is asked about in the class that defines it; a subclass's gets count, but the puts it inherits
are asked about once, in its base.

rkg: `RegisterSink` puts `Graph`, `Oordeel` and `Verbruik` into SQLite. `Graph` and `Oordeel`
come back through `RegisterRepository`, a second class on the same place; `Verbruik` comes
back from `lees_verbruik` as `list[dict]`.
