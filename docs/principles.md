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

Each finding names the principle, the place in the code, the evidence the graph holds,
and ends in a question. Atlas coaches; it does not fix. The author decides what the
answer is, changes the code, and the picture follows.

`atlas view` and `atlas check` read the same graph. The view is the map: your eye finds
the box that should not be there. Check is the list of places on that map that deserve
a second look, found by the engine, which sees every type, call and effect at once.

Atlas is not a linter. ruff, pyright and clippy own the line level: style, a missing
annotation on a private helper, an unused import. Atlas owns the level of types, routes
and places: which doors a package has, what travels between modules, where data rests.
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
- **Rust.** `pub` items at a library crate's root are its doors, and the compiler asks
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
  `reportIncompatibleMethodOverride`. A `# type: ignore[override]` on the line is the
  code saying it knows.
- **Rust.** A trait implementation must match the trait's signature; the compiler
  checks it.

Atlas checks: an override whose annotated return differs from the one its nearest
base declares, grouped per base method.

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
caller. Modules that hold a script, a CLI command, an MCP tool or a test are where the
program starts, and may name places.

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

Atlas checks: a Python function that changes a parameter other than `self`, found from
assignments to its attributes and items and from calls that change it.

goad: `BasePlot.plot_on(other_plot)` sets `other_plot.fig` and `other_plot.ax` from the
outside, then asks it to plot. The question is whether `other_plot` could be told where
to draw by a method of its own.

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
`unwrap_or_else`/`or_else`), and for every comparison of a value with string literals,
the choice it decides. It checks: a parameter (or `self`) tested against the same literal
in two or more places, and a body whose alternatives each produce the same type while it
also takes other steps.

A student's exam project: `run_standard_experiment(args)` tests `args.experiment` in four
places, and `"parallel"` comes back three times. One `Experiment` per name, holding
`use_2d`, `weighted`, `parallel` and the model to build, makes it one place.

In Atlas, `roles::resolve` is such a step: `load | find | infer` is its whole body.
Written inline in `analyze`, the three calls would sit between parsing and assigning
roles, and `atlas check` would ask for their name. Named, `analyze` reads as its
algebra:

```rust
let mut g = lang.parse(repo, package)?;
entries::read_makefile(&mut g);
let arch = roles::resolve(&g, roles)?;
roles::assign(&mut g, arch);
```
