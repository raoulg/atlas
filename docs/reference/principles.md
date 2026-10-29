# Atlas — principles

Atlas draws code as places where data rests and the functions between them. When the
picture looks wrong, the cause is usually a design principle the code does not follow
yet. `atlas-code check` computes those principles from the same graph the picture comes
from, and reports where code and principle disagree.

```bash
atlas-code check            # the repository you are in
atlas-code check ../goad    # another one
atlas-code check --json     # for an agent or a script
```

A finding says in a sentence what the graph shows, and holds the same as data in `--json`:
`about` lists what it is about by the part each plays, each an id the graph holds (a node, a
type, a place) or a name the code writes (a parameter, what a choice tests). A lens's finding is
about the type it `lost`, the `place`, and the methods that `put` and `get` there; an override's
about the overrides that `breaks` its base. The MCP tool `check` gives the same with `json: true`.

A coding agent asks the same through MCP. `atlas-code mcp` serves seven tools over stdio:
`principles`, `check`, `explain` (a principle's section of this page), `routes`, `route`
(what one entry point does, step by step), `view` (the picture opened where the agent points,
written as a page for the human) and `changes` (what an edit did to the routes). In Claude Code:

```bash
claude mcp add atlas -- atlas-code mcp
```

Any other agent takes the same command: `{"command": "atlas-code", "args": ["mcp"]}`.

Each finding names the principle, the place in the code, the evidence the graph holds,
and ends in a question. Atlas coaches; it does not fix. The author decides what the
answer is, changes the code, and the picture follows.

Sometimes the answer is that the code stays as it is: a particle system that updates its pool in
place for the frame rate, a ported module that waits for its rewrite. The author writes that
decision down, with its reason, beside the code it answers, after the language's line comment:

```python
def step(pool: Pool) -> None:  # atlas: ignore[changes] in place for the frame rate, see docs/perf.md
```

```rust
// atlas: ignore[nothing] the log is the point of the call
fn record(event: &Event) {
```

A decision at the end of a line answers that line; one alone on its line answers the first line
below that is not only a comment. For whole paths, a `.atlasignore` at the top of the repository
holds one decision a line: a pattern as `.gitignore` writes one, the principles it answers, and
the reason after `#`.

```gitignore
src/sim/particles.py  changes   # in place for the frame rate, see docs/perf.md
src/legacy/**                   # ported pipeline, rewrite planned
```

A decision that names no principle answers all of them. A finding a decision answers is still
found: it is set aside, counted beside the open questions (`9 questions, 6 set aside`), kept in
`--json` with the decision that answers it as `aside`, and listed with `--all`. A decision that
sets nothing aside, gives no reason or names a principle that is none is asked about below the
summary, so the file does not outlive the code it speaks of. What `atlas-code view` and `changes` ask is
what is open: a finding set aside between two readings is neither raised nor resolved.

`atlas-code view` and `atlas-code check` read the same graph. The view is the map: your eye finds
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
| doors | finding; `__all__` and a star's names export; overloads are a door's signatures, a property its getter; `Any` is unchecked | finding; overload signatures are a door's signatures, a property its getter; `any` is unchecked | forbidden: every signature is typed |
| sums | finding; variant edges, named or written in place | finding; variant edges, named or written in place | finding; an enum's variants, a library's sum (`Result`) handed on |
| overrides | finding; `keeps` from ty, against each base along the method resolution order | finding; what an override takes, which the compiler checks both ways; a type is its members | forbidden: a trait's implementation has the trait's signature |
| places | finding; at the function whose code writes the path's start; `__file__` and `importlib.resources` read what the package ships | finding; at the function whose code writes the path's start; `import.meta.url` reads what the package ships | finding; at the function whose code writes the path's start; `include_str!` is no place, `env!("CARGO_MANIFEST_DIR")` a fixed one |
| changes | finding; a mutates edge per input, to its type: a class, a library's, `list`; a method changes what it is called on as ty finds it defined | finding; a mutates edge per input, to its type: a class, a library's, an array; a method changes what it is called on by that value's type | forbidden: a change is `&mut` or a cell (`&RefCell<T>`) in the signature |
| gives | finding; the call not followed; a key handed on is used; gap (b): a property read | finding; no returns edge; `uses` on the call | forbidden: every function names what it returns; `Result<T, E>` gives `T` |
| choices | finding; a field read through `self` in a class's methods, a parameter by name and type in a module's functions, an `Enum` member | finding; `this` is the value a method runs on, an `enum` member | finding; a field read through `self` in an `impl`'s methods, a variant without fields |
| nothing | finding; what Atlas does not see is named: a `Protocol`'s method, a callback in a field, a class's attribute, a library's call no table describes | finding; the same, with an interface and a `static`; a `Promise` waits and does not fail | finding; a callback in a field is named, and a method called on a type parameter |
| layers | finding; a package's `__init__.py` is a block of its own; a pytest file (`test_*.py`, `*_test.py`, `conftest.py`) is test code | finding; a directory's `index.ts` is a block of its own; a `.test.`, `.spec.` or `__tests__` file is test code | finding; a call among a macro's tokens is a step; `lib.rs` and `mod.rs` are blocks of their own; a `#[cfg(test)]` item or module and a crate's `tests/` are test code |
| lens | finding; takes, returns, has and inherits; the table a statement names, a file by its path or the value that holds it; the steps a value is handed to, a container's `append` among them; gap (a): a field of a library's type | finding; takes, returns, has and inherits; the tables a store's client runs statements on; a `push` onto an array the body declares | finding; takes, returns and has; the tables a store's connection runs statements on; a `push` onto a `Vec`; gap (a): a field of a library's type |
| a place carries a what | a file's path, a request's host | a file's path, a request's host | the type a step gives, a request's host, a program by its constant's value |
| a route ends at a visible place | the places a route reaches; a call not followed is a step | the places a route reaches; a call through a key is a step | the places a route reaches, through a client held in a field |
| zoom by base type and composition | inherits and has edges; what a base's method is handed reaches its overrides | inherits and has edges; the member a parameter is runs | inherits and has edges; a unit struct named is built, a matched value has its variant's type |
| a type comes from a step | ty | a step's return; a cast (`as Config`) is no step's type | forbidden: no cast between unrelated types |

Blind in all three: a statement with its values spliced in (`f"INSERT INTO t VALUES ({x})"`) names
the same table, the same way, as one that binds them.

A finding is a signal to look critically at the code and at what the rule claims; it is never
an oracle and never a target. Each section below says what its rule computes and what it cannot
see, so that no finding means only that the code avoids the forms the rule reads. Atlas is its own
first subject: what `atlas-code check` asks of Atlas is a reviewed list in `engine/tests/review.rs`,
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
  ([PEP 484](https://peps.python.org/pep-0484/)), `*args` and `**kwargs` among them. To keep
  `import package` fast, the exports can resolve lazily with a module `__getattr__`
  ([PEP 562](https://peps.python.org/pep-0562/)).
- **TypeScript.** What the module `package.json` points to exports is the package's doors.
  Annotate what each one gives (`isolatedDeclarations` asks for it); a function handed to a
  call where it is declared (`memo((props) => ..)`) takes its parameters' types from the call.
- **Rust.** `pub` items at a library crate's root, in the modules it makes public with
  `pub mod`, and what it re-exports with `pub use` are its doors, and the compiler asks
  for every type. Atlas has nothing to ask here.

Atlas checks: a Python package called `src`, a Python package that exports nothing at
its root, and a door that does not say what it takes or what it gives.

A Python package's doors are the names `__all__` lists at its root, else every public name the
root binds. `from .core import *` binds what the star brings: the names `core` lists in its
`__all__`, else each public name it defines or binds, and a star of a module that stars another
brings that one's as well. The doors are each exported function, and each public method of an
exported class, its own and those it inherits from the package's classes, with its constructor. A
name with a leading underscore is private; a dunder (`__eq__`, `__add__`, `__getitem__`,
`__call__`) is not: an operator, a builtin or a call runs it for every user of the value, as Rust
runs a trait's `eq` for `==`.

A door says what it takes and gives in the signatures its callers are checked against. Where the
code declares those apart from the body that runs (Python's `@overload`, TypeScript's overload
signatures), the overloads are the door's signatures, and the implementation behind them is not
asked: no caller sees it. A property is its getter: what a reader of the attribute gets. A name
declared again in the same scope is one node, the declaration that runs; its overloads are kept
with it (`Node::overloads`).

In each of those signatures, a parameter says what it takes when its type is written and is not
the unchecked one: `Any` in Python, `any` in TypeScript, which every value passes as and which
passes as every type. Each language's reader marks it (`Param::unchecked`), the picture shows it as
written, and the rule asks one thing of every parameter, `*args`, `**kwargs` and `...rest`
included. A return says what it gives when it is written and is not the unchecked one, where the
language leaves returns to its author (Python, TypeScript); a constructor gives its class. The
finding stands at the first signature that leaves something open, and names each such signature.

What it cannot see. A property's setter and deleter are not asked, and what they do is not in the
graph: an untyped setter behind a typed getter goes unasked in Python (TypeScript gives the setter
the getter's type). `__all__` is read as the strings written in its first assignment at the module's
top: one built from another (`core.__all__ + [..]`) declares only the strings it writes, and what
is added later (`__all__ += ..`) is not read. A star of a module Atlas does not read as code (a library's)
binds nothing it can name. The unchecked type is known by its name as written (`Any`,
`typing.Any`): an alias of it (`JSON = Any`), or `Any` inside another type (`list[Any]`,
`Any | None`), counts as a type that says something, and so does a checked type that says little
(`object`, `unknown`, `dict`). The rule reads what a signature writes; whether it is right is the
type checker's (ty, tsc).

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
  `Result` handed to two functions is a choice each of them makes again, where the caller
  that got it could have made it once.

A sum is a type with an edge to each variant it may be. A union of two or more known types
is one, named (`Result = Fit | Failed`, `type Result = Fit | Failed`) or written in place in a
signature (`def report(r: Fit | Failed)`): a union of the same types is the same sum however
and wherever it is written, so `Fit | Failed`, `Optional[Fit | Failed]`, `Fit | Failed | None`
and TypeScript's `Fit | Failed | undefined` are one sum, and the name a program gives it is its
name. A Rust `enum` is the sum of its variants that carry data: a variant that carries a type of
the program is that type, one that carries none (`Fit { score: f64 }`, `Fit(f64)`) a type of its
own, `Outcome::Fit`. `Result` and `Either` are the library's sums, which arise in a library's call.

The rule asks about a sum that a function of the program gives back, or a library's sum, when
two or more public functions in other modules take it: each of them chooses again. One function
that takes it is not asked. A coproduct is taken apart by a copairing, and that has to stand
somewhere, once: `evaluate(e: Expr)` over `Num | Add`, or a `describe` that gives each caller
what it needs. The finding stands at the sum (where its name is, or where it is first written),
and for a library's sum at the first function that takes it.

What it cannot see. A single type that may be absent (`T | None`, `Optional[T]`, `Option<T>`,
`T | undefined`) is not asked: its consumers ask whether it is there, not which outcome it is,
and an argument that may be left out is a door like any other. A sum inside what is taken
(`Run.outcome: Result`, a field) is not asked through the field, since Atlas does not see
whether a function that takes the `Run` opens its `outcome`. Each function that takes a sum
counts as one that opens it, also when it only hands it on. A union written as a string
(`"Fit | Failed"`), a union of object shapes (`{kind: "fit"} | {kind: "failed"}`) and an enum's
variants without data hold no type the graph knows, and make no sum.

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
(the Liskov substitution principle). An override gives at most what its base promises and takes
at least what its base takes. One that hands back something its base does not promise breaks
that: the caller expected a `Tuple[Figure, Axes]` and got a `Figure`, or `None`. So does one
that takes less: the caller hands an `Event` to a handler that only takes a `Click`. Handing back
a subclass of the promised type is fine, and so is taking a wider type than the base takes.

- **Python.** Type checkers report it as an incompatible override; pyright's
  `reportIncompatibleMethodOverride`, ty's `invalid-method-override`. A
  `# type: ignore[override]` on the line is the code saying it knows.
- **TypeScript.** The compiler holds what an override gives to what its base gives, but checks a
  method's parameters both ways (bivariantly): `override handle(e: Click)` compiles where the
  base takes any `Event`. Its types are structural: a `Circle` with the `area(): number` a `Shape`
  declares is a `Shape`, whether or not it says `implements Shape`.
- **Rust.** A trait's implementation has the trait's signature; the compiler checks it, so the
  careless form cannot be written and Atlas asks nothing.

Atlas checks: an override that cannot stand where a method it overrides stood, grouped per base
method. A class's ancestors are read in its method resolution order, C3's, as Python orders them:
`class D(B, C)` with `B(A)` and `C(X)` runs D, B, A, C, X, a class before its bases and the bases
in the order they are written. Every rule reads which method a class has in this order. A caller
written against any of the ancestors may be handed the override, so it keeps the promise of each
method it overrides, and the finding names the first along the order whose promise it breaks: in
that `D`, where `A.f` gives an `int` and `C.f` a `str`, a `D.f` that gives a `str` breaks `A.f`,
and one that gives an `int` breaks `C.f`. A promise the nearest base already breaks is that
base's question, and is not asked again of what overrides it. A classmethod that gives `Self` is
called through the class a caller names and is handed what the caller hands the base's, so it is
asked like any other method; an initializer (`__init__`, `constructor`) answers to no base. The
finding shows what the base promises, what it takes and what it gives, and what each override
that breaks it says instead. It decides by what the types mean, not by how they are written.

For Python, ty decides, by its own check of an override (`invalid-method-override`) against each
base along the order, what the override takes as well as what it gives, and names the base whose
promise it breaks: `List[Node]` is `list[Node]`, `Optional[Node]` is `Node | None`,
`Annotated[str, "a note"]` is a `str`, a `list[Node]` may stand where a `Sequence[Node]` is
promised, and `handle(self, e: Click)` breaks a `handle(self, e: Event)`. A generic base promises
what its subclass fills in: in `class Boekwinkel(Winkel[Boek])`, the `-> Versie[R]` of `Winkel[R]`
promises a `Versie[Boek]`, whether the base writes its type parameter as `class Winkel[R]` or with
`Generic[R]`, and through any generic base between the two. A parameter the override names
otherwise than its base breaks the promise too, since a caller may pass it by name. A
`# type: ignore[...]` with any code on the first or last line of the override's signature is read
as the bare `# type: ignore` is: ty honours only the bare form and codes of its own (`ty:`), and
Atlas reads the comment itself. Where ty does not know a type (a library the environment does not
hold), it has nothing against it, and neither has Atlas.

For TypeScript, and for a Python override ty has no answer for (of a method of a library's class
read as code), Atlas compares the program's types the two signatures resolve to: each type the override
gives fits one its base promises, and at each place both take a parameter, each type the base takes
there fits one the override takes. A type fits another when it is that type, a subclass of it, or a
variant of a sum the other names; in TypeScript also when it has every member the other declares,
its own and its ancestors': each method by name, with a signature that keeps the other's, and each
field by name, holding a type that fits the other's where the graph has the types of both. A type
with a private method is had only by its subclasses. An array's shape is a promise no type checker
reads (jaxtyping's `Float[Tensor, "batch dim"]`): each array an override gives has a shape its
base allows, where `...` stands for any run of dimensions, a name for any one, and a number for
itself.

What the rule cannot see:

- Where ty does not decide, a type that resolves to none of the program's types (`string`,
  `number`, a library's type) is not compared: an override that gives a `string` where a `number`
  is promised is the compiler's to find.
- A TypeScript parameter is compared by its place, not its name, and its type by the names its
  annotation writes: a `Click[]` is compared as a `Click`, a type parameter (`T extends Event`) and
  an alias of a union as nothing. A generic base's type parameter is not filled in with what the
  subclass hands it, as ty fills it in for Python.
- A member's optionality (`label?: string`), a static member, an index signature and a field the
  graph knows by name only are read as a plain member: a type that lacks an optional member reads
  as lacking it, and two fields of one name and different primitive types read as alike.
- A method that is not one of a class's own (a function assigned in a class body, a mixin applied
  at run time) overrides nothing the graph holds, and a base the graph does not read (a library's
  class not read as code) promises nothing the rule can compare. Where ty finds the promise of such
  a base broken by an override that also overrides a base the graph holds, the finding names the
  nearest base the graph holds.
- Rust's two traits that declare a method of one name are one method of that name in the graph,
  the first implemented; the compiler holds both, so the rule loses nothing, but the picture shows
  one.

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

- **Rust.** `&mut T` says it, and the compiler holds everyone to it. A type that changes
  through a shared reference says it too: `&RefCell<T>`, `&Mutex<T>`, an atomic, or a
  container of one. Atlas has nothing to ask; where `T` is a class of the program, such a
  parameter puts it in the function's codomain, as `&mut T` does.
- **Python.** No signature can say it. So either the object changes itself, through a
  method of its own (`graph.add(node)`), or the function leaves its input alone and
  hands back a new value (`sorted(xs)` next to `xs.sort()`). A method that changes
  `self` is what methods are for.
- **TypeScript.** The same as Python: a `Readonly<T>` parameter can promise no change, but
  nothing promises one. A method that changes `this` is what methods are for; a function
  that fills an array it is handed (`lines.push(..)`) can hand back the array instead.

Atlas checks: a Python or TypeScript function that changes a parameter other than its
receiver, whatever the parameter's type: a class of the program, a library's class, a
`list`, a `dict`, an array, a `Map`. The input's type is in the function's codomain, and
the finding names it as the signature writes it (`list[Row]`). Each input a function
changes is one change: a method that empties the Graph it is handed and its own Graph
changes two, whatever the parameter is called.

A function changes an input when it assigns or deletes one of the input's members or items
(`graph.name = ..`, `counts[k] = ..`, `del graph.nodes[0]`, each target of `a.x, b.y = ..`),
when it hands the input to a function that changes its argument (`setattr`,
`random.shuffle`, `heapq.heappush`, `bisect.insort`), when it calls a method that changes
the value it is called on, and, in Python, when it applies an augmented assignment (`+=`,
`|=`) to one of the language's containers that change in place (a `list`, a `dict`, a
`set`, a `deque`). Whether a call changes what it is called on is decided by that value's
type, never by the method's name: a method of the program changes its receiver when its own
body does; a method of the language's containers changes it when the language's table has
it. In Python that table goes by where ty finds the method:
`list.append` in `builtins`; `clear`, `update`, `setdefault`, `reverse` and `popitem`, which
`list`, `dict` and `set` take from typing's `MutableSequence`, `MutableMapping` and
`MutableSet`; a `deque`'s in `collections`; `os.environ`'s own `setdefault` and `pop`. In
TypeScript it is the methods of an array, a `Map` and a `Set`, on a value declared or built
as one: a parameter, a local, or a field a class declares as one (`this.rows`,
`bag.items`). `http.delete(..)` on a library's client changes nothing.

A change through another name is followed in the order the body runs. A loop variable
stands for an element of what the loop goes over (`for node in graph.nodes: node.status =
..` changes `graph`). A name that a statement of the body binds to another name, or to a
part of one, stands for that after the statement (`g = graph`). A statement of the body
that binds a name to a new value makes every change after it a change to that value:
`graph = copy.copy(graph)`, `rows = list(rows)` and `e = { ...e }` leave the input alone,
and so does `xs = xs + more`. Only a statement of the body's own block decides: one in a
branch or a loop may not run, so a change after it may still change the input and is
asked, and a second name bound in a branch or a loop is not followed.

What the rule cannot see:

- an input without a type: a Python parameter without an annotation, or one ty cannot
  resolve; Atlas cannot name what changes (`doors` asks a door for its annotation);
- a call that hands back the value it is handed: `graph = normalize(graph)` binds a new
  value as far as Atlas knows, so a change after it is not asked;
- an element reached another way than a loop over a name or a part of one: a loop over a
  call (`for n in sorted(graph.nodes)`), a loop that binds a tuple (`for i, n in
  enumerate(..)`), a loop whose body binds its variable anew, a callback (`rows.forEach(r
  => ..)`), a comprehension, or what a call gives (`graph.first().status = ..`);
- the methods of a library's types that change them, beyond the language's containers
  (`io.StringIO.write`, `queue.Queue.put`, a `Date`'s setters): to Atlas they change
  nothing, and `nothing` may ask instead;
- in TypeScript, a member of a value whose class Atlas does not know: `x.rows.push(..)` on
  a value without a type changes nothing.

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
- **TypeScript.** The compiler works out what a function without a return type gives, where a
  reader of its declaration does not see it (`isolatedDeclarations` asks for the type). Atlas
  reads the declaration, so a call on what such a function gives is not followed, as in Python.

Atlas checks: a function or method whose language leaves its return type to the author
(Python, TypeScript), written without one, whose value at least one caller uses. It is the
condition `doors` asks a door on, and a door is asked about by `doors`. A caller uses the value
wherever the call does not stand as a statement: it keeps it, hands it on, returns it, calls a
method on it or tests it. A function handed to a call by a keyword whose function's value
the call uses (`sorted(xs, key=score)`; `PYTHON_USES_HANDED` in `knowledge.rs`) is used there.
The finding names three of the callers that use the value and counts the rest.

What the rule cannot see:

- A property read (`store.size` of an untyped `@property`) is no call in the graph, so an
  untyped property is not asked (an ignored test in `pairs_python.rs`).
- A function handed on in any other way (`map(score, xs)`, TypeScript's `xs.map(label)`, a
  callback a framework calls) is not read as used: what its receiver does with the value, Atlas
  does not see.
- A constructor gives its class and is never asked.

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
choice spread over a body, or over the functions of a module, is neither.

Two shapes of the same problem:

- **A value handed in is tested in several places.** `mode == "parallel"` here,
  `mode in {"weighted", "parallel"}` there, in one body or in two functions: what
  `"parallel"` means is spelled out again in each place, and a new mode is a change in all
  of them (Fowler's *repeated switches*). Made once, the choice becomes a table or a type
  that says what each case stands for. A test of an enumeration's member
  (`mode is Mode.FAST`) is the same test as one of a string: the type names the cases, and
  the choice is still spelled out in each place.
- **Alternatives sit between other steps.** `load(p)` or `find(g)` or `infer(g)`: three
  ways to get one value, in the middle of a body that also does other things. Where the
  value comes from is a step of its own; given a name, the body reads as its steps
  again.

- **Python.** A `Literal["a", "b"]` or an `Enum` for the cases, and one dict from case
  to what it means: `EXPERIMENTS: dict[str, Experiment]`, with `Experiment` a dataclass
  ([PEP 557](https://peps.python.org/pep-0557/)) holding the facts each `if` asked
  about, or methods on the `Enum` that each `match` the value they run on once. A choice
  between steps becomes a function whose body is that choice, written with `match`
  ([PEP 634](https://peps.python.org/pep-0634/)) or `if`/`elif`.
- **TypeScript.** An `enum` (or a union of string literals) for the cases, and one
  `Record<Mode, Plan>` from case to what it means. A choice between steps becomes a function
  whose body is that choice.
- **Rust.** An `enum` for the cases with one `match`, and methods on the enum for what
  each case means. A choice between steps becomes a function whose body is the `match`
  (or the `unwrap_or_else` chain).

Atlas records, for every call, the arms of the choices it sits in (`if`/`elif`/`else`,
`match`, `try`/`except`, `a if c else b`, `x or y`; TypeScript's `if`, `switch`,
`try`/`catch`, `c ? a : b`, `x || y`, `x ?? y`; Rust's `if`, `match` and
`unwrap_or_else`/`or_else`), the name the body keeps its value by, through the arms of a
choice (`g = a() if c else b()`), and for every comparison of a value with its cases, the
choice it decides.

A case is a string literal or a member of a type: `Mode.FAST`, `Mode.Fast`, `Mode::Fast`.
Python reads a member where ty says its qualifier is a class, or where the qualifier names one
of the program's classes; TypeScript where the qualifier names one of the program's enums or
classes; Rust where the qualifier is written as a type's name, in upper camel case, which the
compiler's naming lints hold every type to. In a `match` or a `switch`, a member is a case on
its own or among alternatives (`Mode::Fast | Mode::Slow`); a variant taken apart
(`Shape::Circle(r)`, `case Point(x=0)`) is a sum unwrapped, which `sums` reads, and no case.
The value tested is the value as the body was handed it: a name the body binds once to a value
it reads in place (`m = args.mode`, `const l = this.level`, `let m = &self.mode`) is that
value, so `m == "fast"` tests `args.mode`.

It checks two things.

- **One value tested against the same case in two or more places.** A value is the same
  across bodies where the graph says so. A field of a class read through the value its
  methods run on (`self.mode` in Python and Rust, `this.mode` in TypeScript) is one value in
  every method of that class. A parameter of one name and written type (`mode: str`), and
  what is read through its fields, its items at a literal and, in Rust, its calls that are
  handed nothing (`args.mode`, `parts[0]`, `n.kind()`), is one value in every function of
  its module, methods among them. Any other value a body is handed, the value a method runs
  on itself among them (`match self` in an enum's own methods), is one value in that body.
  A place is a choice, or a comparison that decides none on its own line, in one body. The
  finding stands where the value is defined: at the class, or at the first function that
  tests it. It names every function that tests the value and the cases that come back, and
  its evidence lists each test with its line.
- **Alternatives that get one value by different steps, between other steps.** Two or more
  arms that each make a value of one type for one later step of the body, by the name it is
  kept by or handed straight on, each by a step of its own (`load(p) if c else infer(p)`).
  Arms that each make the value by the same step (`Message("on") if c else Message("off")`)
  are that one step with a choice of what it is handed, the same arrow as
  `Message("on" if c else "off")`: by the law of the coproduct, `h ∘ [f, g] = [h ∘ f, h ∘ g]`.
  They choose an argument, not where the value comes from, and are not asked. A choice whose
  value the body returns, or whose arms each finish their own case, is the body's name, not
  a step without one.

What the rule cannot see:

- **Sameness beyond the class and the module.** A field read through another value of the
  class (`other.mode`), the methods of a subclass or a mixin, a parameter of the same name in
  another module, and a value handed on to another function under a new name are each a
  value of their own. A value read through a call that is handed something, or through
  another name (`self.nodes[t].kind`), is its body's own.
- **A parameter's name and written type are all it knows of sameness across functions.** Two
  functions whose `n: Node` are different nodes test one value to the rule; two that write
  one type differently (`TsNode` and `TsNode<'t>`) test two.
- **A value made from the one handed in.** `mode.lower() == "fast"` in Python and TypeScript
  is no comparison of `mode`; Rust reads a call on a handed value as a value of its own, so
  `self.mode.as_str()` and `self.mode` are two. A local bound twice, or bound to what a call
  or an operator gives (`m = args.mode or "fast"`), stands for no handed value.
- **Cases it does not read.** A number compared (`level == 2`) is no case: a number is rarely
  a type code, and a test of a threshold or a count is written the same way. A member of a
  library's enumeration is no case in TypeScript, nor in Python where ty does not know the
  library. Rust's `matches!` is read for its strings, not its members.

A student's exam project: `run_standard_experiment(args)` tests `args.experiment` in four
places, and `"parallel"` comes back three times. One `Experiment` per name, holding
`use_2d`, `weighted`, `parallel` and the model to build, makes it one place.

In Atlas, `file_direction` is such a step: which way a file argument goes (`Reads |
Writes | src | dst | what the operation does`) is its whole body. Written inline in
`file_calls`, the five arms would sit between reading the call's literals and placing its
file, and `atlas-code check` asks for their name. Named, the body reads as what it does:

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
a field, an item or a loop. Then the change belongs where a reader sees it. A global the
function changes is shown: a value a module keeps between calls is a place,
`Global(module.name)`, written by whoever changes it and read by whoever uses it. Atlas draws it
and does not judge it, since a global is sometimes the right choice. A library's module is code,
so what it holds is the place: a function that sets `os.environ["DB"]` for another to read
writes `Global(os.environ)`, and a call of `os.path.join` reads nothing. In Rust the place is a
`static mut`, or a `static` of a type that changes in place (a lock, a cell, an atomic); one a
function's body declares is named after the function. Ending the program leaves something
behind as an error no one catches does: `sys.exit(..)` and `os._exit(..)`, Node's
`process.exit(..)` and Rust's `std::process::exit(..)` count as `raise`, `throw` and `panic!`
count (`ENDS_THE_RUN` in `knowledge.rs`).

- **Rust.** `&mut T` and `-> Result<()>` say what a function changes and that it may fail.
  What stays hidden is a value behind a `RefCell`.
- **Python.** A function that fills a value it is handed (`for node in graph.nodes:
  node.status = ...`) can be a method of that value, or hand back what it makes.
- **TypeScript.** The same as Python; a function that sets fields on the elements of an
  array it is handed can hand back the array.

Atlas checks: a function another function of the project calls, that gives nothing back
(Python `-> None`, TypeScript `void`, Rust no return type), whose body does not raise, throw,
panic or end the program and whose return type does not say it may fail (`Result`), and where
nothing it runs (what it calls, what it builds, the overrides a call may reach, down to the
end) touches a place, changes a value or a parameter, fails, or calls a function it was handed.
`async` changes none of this: an `async def` and a `Promise<void>` say that nothing arrives
later, and a `Promise` rejects where what it runs throws, as any call may. A door or an entry
is asked as any other function: its callers in the program get the same arrow. A method that is
only declared (abstract, of a `Protocol`, an interface's, a trait's without a body) has no body
to ask about; what calls it is asked.

The question depends on what Atlas sees of what the function runs:

- **All of it.** Everything it runs is code Atlas reads, or calls whose effect the tables in
  `knowledge.rs` state: what crosses into a place, what changes a value, what ends the program,
  and the language's own (Python's builtins and `sys`, TypeScript's globals), which compute a
  value and nothing more. The question is whether the function is dead, or changes a value it
  is handed in a way no call shows.
- **Not all of it.** It runs something whose effect Atlas does not model, and the finding names
  it with its line: a library's call whose value goes nowhere and that no table describes
  (`logger.info(..)`, `time.sleep(..)`, `random.seed(..)`, torch's `x.mul_(..)`); a call Atlas
  cannot follow (a callback kept in a field, `self.on_done()`; a call on a value without a type);
  a method only declared that nothing in the graph implements (a `Protocol` or an interface that
  a class fills in without naming it); a value a class keeps, which the function changes
  (`Counter.total += 1`). The question is what that leaves behind (a log, a change in a
  library's or a class's value, a wait), not whether the function is dead.

What the rule cannot see:

- A change of a value handed in through a field, an item or a loop (`node.status = ..`,
  `d[k] = v`) is no call: the function reads as all seen, and the question names such a change.
- A library's call whose value the body keeps is read as computing that value and nothing more
  (`n = buf.write(s)`): the function reads as all seen.
- The language's own is read as the tables describe it, which is not all it does:
  TypeScript's `Object.assign(target, ..)` changes its target, Python's `sys.setrecursionlimit`
  changes the interpreter.
- A decorator that wraps a function (click's `@group`) makes a call to it run the framework's
  code; Atlas reads the body.
- A Rust method called on a value of a type parameter (`fn run<T: Task>(t: &T) { t.run() }`) is
  not followed: it is named as a call Atlas cannot see.

rkg: `stempel_herkomst(graph, herkomst) -> None` sets the author, status and date on every
node of the graph it is handed, through a loop. Atlas sees a function into nothing; what it
does is change the Graph. As a method of the graph, the change shows as what it is.

## The packages of a part depend on one another in one direction.

Id: `layers`.

The code of a part, quotiented by package, is a preorder: a package depends on another when its
code calls, builds, imports, extends or requests what the other holds, or takes, returns or holds
one of its types. Where that preorder is a partial order, the packages have layers: each can be
read, tested and replaced with only what lies below it. A cycle has no below. The packages in it
are one package in all but name, and the link that closes it is usually one call against the flow.

- **Python.** A package that imports from the package that imports it, often inside a function
  to get past the import cycle, is the sign; so is a module that imports from its own package's
  `__init__.py`, which imports the module. What both need moves down into a module of its own.
- **Rust.** A crate's modules may use one another freely; the compiler stops no cycle, also not
  the one between a module and the file that declares it (`super::`). What belongs to the lower
  module moves there, and the higher one calls it.
- **TypeScript.** An interface module that also picks the implementation (`kiesBron` beside
  `BronLike`) imports the implementations that import it. The choice moves up, to whoever uses
  the interface.

Atlas checks: the blocks are the package quotient the picture draws, at each of its levels: the
parts of a graph beside one another, the top-level packages of a part, and under each package the
packages below it, with the package's own module beside them as a block of its own (`orders
itself`: the code of the package in none of them, its `__init__.py`, `lib.rs` or `mod.rs`,
`index.ts`). A level with one package and no code of its own is passed through, as the picture
passes it. A block depends on another by the calls, constructions, imports, base classes and
requests of its code, and by the types its signatures and fields name (what a function takes and
returns, what a type holds, a variant of a sum): a cycle through types alone is a cycle. Test code
is a way in from outside, not a layer: a test, and the code of a test module (a pytest file, a
TypeScript test file, a Rust `#[cfg(test)]` item or module and a crate's `tests/`). Atlas names each
cycle with its links, the thinnest first.

What it cannot see: a package's own module that holds no code the graph keeps (only imports, or
values no function changes) is no block, so what is imported from it alone (`from pkg import
__version__`) links nothing. Test code is known by where it stands; a test helper in a module of
the program is the program's. The imports of a Rust test module are the tests', and the graph
keeps none of them for the module around it. A request from one part to another is in the graph
of the repository as a whole; `atlas-code check` reads each part alone.

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

The place a lens is on is the place the code names: the table a statement names (a key-value
store's key), a file by its path or by the value that holds it (`Disk(Log.path)`, the path a
store is made with). Two tables of one database are two places, and two files are two places. A
class that logs visits into `visits` and lists names from `members` writes one table and reads
another, and is a lens on neither. Where the code names no such place there is no lens to ask
about: a statement whose table is filled in at run time, a database whose statements Atlas does
not read (a graph database's queries), and a file known only by its format: what
`json.dump(visit, out)` writes to a stream handed in is a JSON file, not a file the code names.

A get gives a value back from the place. An operation that reads the place and gives nothing
(`add(visit) -> None` that reads the list in its file, appends the visit and writes the list
back) reads on its way to writing, and is a put: a put `A × S → S` takes the place it changes.
A log that only appends is an end product. A constructor that reads the place gives its class
from it, and is a get: a `@classmethod` `Roster.load(path) -> Roster`, `Roster::load` in Rust,
a TypeScript `static load`. A touch that tests or makes a place's shape (`path.exists()`, a
directory made, a table created) neither puts there nor gets.

What comes back may come back inside what a get gives: a lens composed with the projection to a
field is a lens on that field. A register that puts a `Verificatie` and gives back the
`Definitie` that holds it (`Definitie.verantwoording: ... | Geverifieerd`,
`Geverifieerd.verificatie: Verificatie`) gives back what it put, and so does a get that gives the
class's own type, which holds what the class keeps (`reread(self) -> Roster` with
`Roster.members: list[Member]`). A store that updates one field (`Boek.auteur`) or adds one node
to a graph puts through such a composite: the key that picks the value, the field or the list that
holds it, and the variant it is. A sum given a name is the sum it names: `nodes: list[Node]` with
`Node = Begrip | Bron` (a Python alias, a TypeScript `type`) holds a `Bron` as `list[Begrip | Bron]`
does, and so does a Rust `enum Node`. A subclass is a variant of a sum left open: a get that gives
`list[Post]` gives back the `Notice(Post)` a put took. A TypeScript constructor's `readonly`
parameter is a field of its class, as one with an access modifier is.

What a put takes is what reaches the place. A tile store that looks a tile up by a `Punt` and,
when none holds it, fetches the area around it through its own `materialiseer(gebied: BBox)`,
puts the `BBox`, not the `Punt`: the point is the key the tile is found by, as a `WHERE` is the
key a row is found by. A statement puts what it is handed only where it stands among the values
the statement writes, an `INSERT`'s values and an `UPDATE`'s `SET`: a `DELETE` and an `UPDATE`
whose `SET` holds only literals are handed only the key their rows are found by
(`DELETE FROM visits WHERE member = ?`), and put none of it. An `INSERT .. SELECT` that copies
another table is handed nothing, and puts none of it either.

Atlas checks: a class whose public operations both put to and get from one place (a table, a
named file; through what they call), for each type of data a put takes that no get of the class
gives back from that place, as it is or inside what the get gives (a field, an element, a variant,
a subclass, followed through `has`, `variant` and `inherits` edges). A class that only reads a
place (what comes in from the world) or only writes it (an end product) is no lens on it. Where
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
is handed), unless it is a statement that writes none of what it is handed; a call Atlas cannot
follow may put what it is handed wherever its function puts; a value no step makes
(`for r in readings`, `x = p.field`) is part of any parameter whose type holds its type, as the
parameter is declared or handed; and a parameter that is neither handed to a step by name nor
holds such a value reaches the place when any step puts there. A loop variable is an element of what it goes over, in every language: ty types it in
Python; in Rust and TypeScript it has the element type of the sequence the body declares
(`readings: &[Reading]`, `readings: Reading[]`), so a parameter only iterated is followed through
its elements.

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

What the rule cannot see:

- **Values.** The law is on values, the rule on types. It checks a necessary condition of
  `get(put(a, s)) = a`, where the place is the right one: that what goes in can come back as the
  type it went in as. A store whose get builds `Visit(member=day, day=member)` from the row its put
  wrote gives back a `Visit`, and passes. A class that puts into `visits` and gets from `archive`
  is a lens on neither table and is not asked, though nothing it puts comes back.
- **Which placeholder a value fills.** The graph has the statement, not what the call hands each
  placeholder: an `UPDATE .. SET name = ? WHERE id = ?` handed a value and a key, or an
  `INSERT .. SELECT .. WHERE day = ?`, may put any of what it is handed, and a parameter it uses
  only as the key is asked about as a put.
- **Which database.** A statement names its table, not the file its connection opens: two
  databases that each hold a table of one name are one place.
- **A table a call names by argument.** A library call that names its table as an argument
  (`frame.to_sql("readings", con)`) rather than in a statement gives no place.
- **A put a subclass inherits.** It is asked about in its base, beside the base's gets: a base that
  only puts, whose subclass gets the place back as another type, is asked nothing.
- **A field of a library's type** (*gap* in Python and Rust): their readers draw no `has` edge to
  a library's type, so a get that gives an `Entry` with `amount: Fraction` is not seen to give the
  Fraction back, and a store that puts the Fraction is asked about it.

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

What a prophecy must not be is a dependency. Code that is built and calls, builds, extends,
requests or takes a prophecy would run on a stub: it raises, or does nothing, in production.
Prophecies lean on one another freely, and test code may lean on the code it is written for
before it is built.

- **Python.** `@prophecy(supersedes="pkg.module.Old")`, a no-op decorator in the project; Atlas
  matches it by name. The marker is always a call, `@prophecy()` where nothing is superseded: the
  decorator is what `prophecy(..)` gives, and ty types a class under a bare `@prophecy` as that, so
  the prophecy would not draw the shape of its vision. The decorator lives in a module that imports
  nothing of the project, or the files it marks cannot import it. The body is `raise NotImplementedError` where the function gives a value (ty
  refuses an empty `...` body there, outside stubs and abstract methods) and `...` where it gives nothing; ty
  checks the signatures, and `prophetize` writes the bodies so.
- **Rust.** `#[doc = "prophecy: supersedes=pkg::Old"]` on the item, with `todo!()` for a body: it
  compiles, and the types are checked in full.
- **TypeScript.** `/** @prophecy supersedes=pkg.Old */` before a `declare function`, a `declare
  class`, an interface or a type. A decorator would reach classes and their members only.

`supersedes` names what the prophecy replaces by its id, or by the end of it when one node only
ends so; `::` and `.` part a path alike. A path in Rust's form starts where Rust starts it, from
the prophecy's module: `crate::` (or the crate's own name) at the crate, `self::` at the module,
`super::` at the module above it. `veiled` says the place is known and the shape is not,
`sealed="#144"` that the shape waits on an issue.

Atlas checks: a built node with a call, an instantiation, an inheritance, a request, a parameter,
a return, a field or a variant of a prophecy (a member of a prophecy counts as the prophecy), unless
it is test code: a test, or code in a test module (a pytest file, a TypeScript test file, a Rust
`#[cfg(test)]` item or module and a crate's `tests/`), as `layers` reads it. And a prophecy whose
`supersedes` names nothing the code holds (a false prophecy), or ends the ids of several of the
program's nodes (a library's do not count): then it does not say which it replaces, and Atlas
names them.

What it cannot see: a test helper in a module of the program is built code. A Rust path through a
`use` (`supersedes=store::save` where `store` is a name the module imports) is read as written, not
through the import, and `self::` and `super::` start from the file's module, also inside an inline
`mod`. A request to a prophesied route of another part is in the graph of the repository as a
whole; `atlas-code check` reads each part alone, and asks about a request only where a part serves it
itself (a TypeScript page that fetches its own routes).
