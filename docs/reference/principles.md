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

Each principle is stated once, on the graph, for every language. A language decides how
much of it can be traced: where the careless form says to any reader exactly what the careful
one says, no fact in the graph tells them apart, and in that language the difference does not
exist. A language whose signatures say more (a Rust parameter that is `&mut`, a sum type the
compiler makes you unwrap) lets Atlas find what it cannot find in one whose signatures say less.
A new language brings a reader, never a principle. The pairs in `engine/tests/pairs` show, per
language, which principles it traces and by which fact.

Per principle and language, the fact that tells the careful form from the careless one. *Forbidden*:
the language cannot write the careless form, so there is nothing to lay bare. *Gap*: the language
lets it be traced and Atlas does not yet; each gap is an ignored test in `pairs_<language>.rs`,
(a) where Atlas misreads the careful form, (b) where it draws the careless one as careful or drops it.

| Principle | Python | TypeScript | Rust |
|---|---|---|---|
| doors | finding; `__all__` exports | finding on an untyped export or one typed `any` | forbidden: every signature is typed |
| sums | finding; variant edges | traceable (a union alias) | finding; a library's sum handed on (`Result`) is asked |
| overrides | `keeps` from ty; finding | forbidden: the compiler holds an override to its base | forbidden |
| places | finding; at the function whose code writes the path's start; `__file__` and `importlib.resources` read what the package ships | finding; at the function whose code writes the path's start; `import.meta.url` reads what the package ships | finding; at the function whose code writes the path's start; `include_str!` is no place, `env!("CARGO_MANIFEST_DIR")` a fixed one |
| changes | mutates edge; finding | mutates edge; finding | forbidden: a change is `&mut` in the signature |
| gives | finding; the call not followed | no returns edge; `uses` on the call | returns edge; `Result<T, E>` gives `T` |
| choices | finding | finding | finding |
| nothing | finding | finding | finding |
| layers | finding | finding | finding; a call among a macro's tokens is a step |
| lens | finding; takes, returns and has; the steps a value is handed to, a container's `append` among them | finding; takes and returns; the tables a store's client runs statements on; a `push` onto an array the body declares | finding; takes and returns; the tables a store's connection runs statements on; a `push` onto a `Vec` |
| a place carries a what | a file's path, a request's host | a file's path, a request's host | the type a step gives, a request's host, a program by its constant's value |
| a route ends at a visible place | the places a route reaches; a call not followed is a step | the places a route reaches; a call through a key is a step | the places a route reaches, through a client held in a field |
| zoom by base type and composition | inherits and has edges; what a base's method is handed reaches its overrides | inherits and has edges; the member a parameter is runs | inherits and has edges; a unit struct named is built, a matched value has its variant's type |
| a type comes from a step | ty | a step's return; a cast (`as Config`) is no step's type | forbidden: no cast between unrelated types |

Blind in all three: a statement with its values spliced in (`f"INSERT INTO t VALUES ({x})"`) names
the same table, the same way, as one that binds them.

A finding is a signal to look critically at the code and at what the rule claims; it is never
an oracle and never a target. Each section below says what its rule computes and what it cannot
see, so that no finding means only that the code avoids the forms the rule reads. Atlas is its own
first subject: what `atlas check` asks of Atlas is a reviewed list in `engine/tests/principles.rs`,
each finding with why it stands, and the test shows when the list changes.

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

A program `pyproject.toml` declares is a command, also where its package exports the function
(a flat module's public `main`); exported, it is a door as well, and asked as one.

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

Atlas checks: an override that cannot stand where the method of its nearest base stood,
grouped per base method. The finding shows what the base promises, what it takes and what it
gives, and what each override that breaks it says instead. It decides by what the types mean,
not by how they are written. For Python, ty decides, by its own check of an override
(`invalid-method-override`), what the override takes as well as what it gives: `List[Node]` is
`list[Node]`, `Optional[Node]` is `Node | None`, `Annotated[str, "a note"]` is a `str`, and a
`list[Node]` may stand where a `Sequence[Node]` is promised. A generic base promises what its
subclass fills in: in `class Boekwinkel(Winkel[Boek])`, the `-> Versie[R]` of `Winkel[R]` promises
a `Versie[Boek]`, whether the base writes its type parameter as `class Winkel[R]` or with
`Generic[R]`, and through any generic base between the two. A parameter the override names
otherwise than its base breaks the promise too, since a caller may pass it by name. Where ty does
not know a type (a library the environment does not hold), it has nothing against it, and neither
has Atlas. For Rust and TypeScript, whose compilers already hold an override to its base, Atlas
compares the program's types the two signatures give: each type the override gives is one its
base promises, a subclass of one, or a variant of a sum it promises. `anyhow::Result<Page>` keeps
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

Atlas checks: a read or a write whose file or URL, once its path is whole, starts at something
the code holds: a literal or a constant (`open("out.csv")`, `Path("data") / name`,
`f"out/{x}.png"`, `DATA / "raw"` with `DATA = Path("data")`), or the directory a Rust crate was
built in (`env!("CARGO_MANIFEST_DIR")`, read at run time: the source directory of the machine the
program was built on). The finding stands at the function whose own code writes what the path
starts at, wherever the call that opens the file is: `default_rows()` that calls
`read_rows(Path("data/rows.csv"))` decides the file as much as one that calls
`Path("data/rows.csv").read_text()`, and `read_rows`, which takes the path, is not asked. The path
is followed out through the callers one call at a time, and through the object a caller builds
with it (`Cache(Path("warm")).keep(..)` decides where `keep` writes). The evidence names the call
that opens the file and, where that is another function's, the function (`read_text(data/rows.csv)
in read_rows`).

The code does not decide:

- a path that starts at a value from elsewhere (`root / "out.csv"`, `root.join("Cargo.toml")`):
  the caller decides;
- one that starts at what the environment says (a variable, the user's home, the system's
  temporary directory);
- a default whoever calls the function or starts the program can override. A parameter's default
  (`path: Path = Path("out.csv")`), an option of the command line, and what the code falls back to
  where a caller or the environment says nothing (`os.environ.get("STORE", "out.csv")`,
  `os.getenv("STORE") or "out.csv"`, `process.env.STORE ?? "out.csv"`,
  `env::var("STORE").unwrap_or_else(|_| "out.csv".into())`, `path or Path("out.csv")`) say the same
  thing: whoever starts the program can choose, and the code names what it uses when nobody does.
  They answer alike, and the picture draws the variable, a place the code does not name, beside
  the file the code takes by default;
- a file that ships with the package, read from where its code lies (`Path(__file__).parent /
  "template.html"`, `importlib.resources.files("pages")`, `new URL("./template.html",
  import.meta.url)`): it is part of the package, not a place its user chooses. Written there, a
  file lands in the directory the package is installed in, a place the code decides, and is asked.
  Rust compiles such a file in (`include_str!("template.html")`), and it is no place at all.

Where the program starts may name places: an entry itself (a CLI command, a route, an MCP tool, a
test, a binary's `main`), a file that runs as a whole (a script, a notebook, a test file) with the
code it defines, and what a module that runs as a command keeps to itself (the `main` a TypeScript
`bin` runs, which the module does not export). A program `pyproject.toml` declares is a command,
also where its package exports the function. A function beside an entry is library code all the
same: a helper in the module of a CLI command, or a function in a Rust file whose `#[test]`s sit
below it, takes its place from whoever calls it. So is a function of a module that runs as a
command when it is run itself (`if __name__ == "__main__":`), where the module offers it: the
module can be imported, and a notebook that calls the function gets the place it picked.

What the rule cannot see:

- A path a function the code calls makes for it (`path_for("x").write_text()`, where `path_for`
  gives `Path("data") / name`) is read where it is used: the finding stands at the caller, not at
  `path_for`, whose code writes the literal.
- A value Atlas does not follow (a call it does not resolve, a key looked up at run time, a field of
  an untyped value) is unknown: it decides nothing, and what the code falls back to beside it is
  taken for its default.
- A path is followed out twelve calls at most, and to a bounded number of the calls that complete
  it (64): a function further out, or past the bound, that hands it a place is not asked.
- A database is not asked: the file a connection opens (`sqlite3.connect("app.db")`) and the tables
  a statement names are drawn, but the rule asks of files and URLs.
- A helper in a Rust file of tests (`tests/*.rs`) that finds its fixtures through
  `CARGO_MANIFEST_DIR` is asked, though the file runs as a whole, as a Python test file does.
- TypeScript's parameter defaults are not evaluated: the picture draws the parameter, where
  Python's draws the file the default names.

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
an item or a loop; a library's own state. Then the change belongs where a reader sees it. A
global the function changes is shown: a value a module keeps between calls is a place,
`Global(module.name)`, written by whoever changes it and read by whoever uses it. Atlas draws it
and does not judge it, since a global is sometimes the right choice. A library's module is code,
so what it holds is the place: a function that sets `os.environ["DB"]` for another to read
writes `Global(os.environ)`, and a call of `os.path.join` reads nothing. In Rust the place is a
`static mut`, or a `static` of a type that changes in place (a lock, a cell, an atomic); one a
function's body declares is named after the function.

- **Rust.** `&mut T` and `-> Result<()>` say what a function changes and that it may fail.
  What stays hidden is a value behind a `RefCell`.
- **Python.** A function that fills a value it is handed (`for node in graph.nodes:
  node.status = ...`) can be a method of that value, or hand back what it makes.
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

What comes back may come back inside what a get gives: a lens composed with the projection to a
field is a lens on that field. A register that puts a `Verificatie` and gives back the
`Definitie` that holds it (`Definitie.verantwoording: ... | Geverifieerd`,
`Geverifieerd.verificatie: Verificatie`) gives back what it put. A store that updates one field
(`Boek.auteur`) or adds one node to a graph puts through such a composite: the key that picks
the value, the field or the list that holds it, and the variant it is. A sum given a name is the
sum it names: `nodes: list[Node]` with `Node = Begrip | Bron` (a Python alias, a TypeScript
`type`) holds a `Bron` as `list[Begrip | Bron]` does, and so does a Rust `enum Node`. A
TypeScript constructor's `readonly` parameter is a field of its class, as one with an access
modifier is.

What a put takes is what reaches the place. A tile store that looks a tile up by a `Punt` and,
when none holds it, fetches the area around it through its own `materialiseer(gebied: BBox)`,
puts the `BBox`, not the `Punt`: the point is the key the tile is found by, as a `WHERE` is the
key a row is found by.

Atlas checks: a class whose public operations both write and read one place (a database as a
whole, a file by itself; through what they call), for each type of data a put takes that no
get of the class gives back from that place, as it is or inside what the get gives (a field, an
element, a variant, followed through `has` and `variant` edges). A class that only reads a place
(what comes in from the world) or only writes it (an end product) is no lens on it. Where
another class gets that type from the same place, Atlas says which: the lens is then split over
two classes. A put is asked about in the class that defines it; a subclass's gets count, but the
puts it inherits are asked about once, in its base.

A put takes a parameter when the parameter's value, or what the body's steps make of it, is
handed to a step that puts what it is handed into the place: one whose own parameters reach it,
in turn. A step that reads and puts nothing it is handed there gives what the place holds, so
what it is handed is a key and goes no further. What an operation hands another operation of its
class that takes that type is put there, and asked about there. Where the graph does not show
where a value goes, Atlas does not guess that it goes nowhere: a call of the operation's own into
the place may be handed any of its parameters (the graph does not say what a call into the world
is handed), a call Atlas cannot follow may put what it is handed wherever its function puts, a
value no step makes (`for r in readings`, `x = p.field`) is part of any parameter whose type holds
its type, as the parameter is declared or handed, and a parameter that is neither handed to a step
by name nor holds such a value reaches the place when any step puts there. A loop variable is an
element of what it goes over, in every language: ty types it in Python; in Rust and TypeScript it
has the element type of the sequence the body declares (`readings: &[Reading]`,
`readings: Reading[]`), so a parameter only iterated is followed through its elements.

A call that changes the value it is called on is a step that makes that value anew from what it
is handed, `T × A → T`: `rows.append(row)`, `rows.push(row)`, `seen.add(key)`. What is made of a
parameter and gathered into a container that way, then written, reaches the place; what is
gathered into a container that is not written does not. Which calls change what they are called
on is a fact of the language or the library, never of a method's name alone: in Python, a method
ty says is one of `builtins`' mutable containers (or a library's, networkx's graphs); in Rust, a
method of a std collection on a value the body declares or makes as one (`Vec`, `String`,
`VecDeque`, the maps and sets); in TypeScript, the language's own on a name the body declares or
builds as an array, a `Map` or a `Set`. A method of the program's own changes its receiver when
its body does. One that returns a new value (`Frozen.append(row) -> Frozen`) changes nothing, and
neither does an observation whose result is dropped (`path.exists()`).

rkg: `RegisterSink` puts `Graph`, `Oordeel` and `Verbruik` into SQLite. `Graph` and `Oordeel`
come back through `RegisterRepository`, a second class on the same place; `Verbruik` comes
back from `lees_verbruik` as `list[dict]`.

## Code that is built does not lean on code that is not.

Id: `prophecy`.

A design can live in the tree as sketch code, marked as not built yet: a *prophecy*. It commits
to a type, an argument and a caller, as a note cannot, and the distance between it and the code
that exists is the list of what is left to build. A prophecy is a todo, not a finding: its empty
bodies are what it is, and the other principles leave it alone. What it `supersedes` stays, and
is still asked about, with the prophecy as the answer.

What a prophecy must not be is a dependency. Code that is built and calls, builds, extends or
takes a prophecy would run on a stub: it raises, or does nothing, in production. Prophecies lean
on one another freely, and a test may lean on the code it is written for before it is built.

- **Python.** `@prophecy(supersedes="pkg.module.Old")`, a no-op decorator in the project; Atlas
  matches it by name. The body is `raise NotImplementedError` where the function gives a value (ty
  refuses an empty `...` body there, outside stubs and abstract methods) and `...` where it gives nothing; ty
  checks the signatures, and `prophetize` writes the bodies so.
- **Rust.** `#[doc = "prophecy: supersedes=pkg::Old"]` on the item, with `todo!()` for a body: it
  compiles, and the types are checked in full.
- **TypeScript.** `/** @prophecy supersedes=pkg.Old */` before a `declare function`, a `declare
  class`, an interface or a type. A decorator would reach classes and their members only.

`supersedes` names what the prophecy replaces by its id, or by the end of it when one node only
ends so. `veiled` says the place is known and the shape is not, `sealed="#144"` that the shape
waits on an issue.

Atlas checks: a built node with a call, an instantiation, an inheritance, a parameter, a return
or a field of a prophecy (a member of a prophecy counts as the prophecy), and a prophecy whose
`supersedes` names nothing the code holds (a false prophecy).
