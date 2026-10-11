# Atlas — working instructions

## The goal

Atlas is not a code viewer. It is a shared language between a human and an agent for
building software together. The human has sensors the agent lacks (a picture, a
name, a shape that looks wrong at a glance); the agent has sensors the human lacks
(every type, call and effect at once). Informal language between the two is sloppy
and expensive for both. Atlas replaces it with a small piece of mathematics that has
one picture per construct, so that pointing at a box and saying "this should not
exist" is a statement both sides can act on, and so that the agent can see what the
human means from where the human is pointing.

The rigorous half and the visual half are the same structure. Read `docs/design.md`
§0 before changing either.

## The shared language, in six sentences

1. Objects are places where data rests: Memory(type), File(path), Database(table),
   Network(url), Process(command), Screen(human), Library(what a dependency produces).
   Every place carries a *what*.
2. Morphisms are functions. Domain: parameters plus what the body consumes.
   Codomain: what is returned plus what is written, sent or run. The author's names
   are the labels.
3. Composition is call order. A route starts at an entry point and ends at a place;
   both must be visible.
4. Zoom is a quotient on types: by base class, by composition, by module. Zooming
   out coarsens, zooming in refines one family at a time. The picture stays the same
   picture.
5. Filters are fibers: the part of the graph where a predicate holds. One map gives
   both zoom and filter: its quotient is the overview, a fiber is one class alone.
   Fiber and quotient do not commute; order in a view is meaning.
6. A step's own morphism and what it transitively reaches are different quantities
   and are never merged.

## Rules that follow

- **The engine knows languages and libraries, never a project.** Nothing about rkg,
  vectormesh or Atlas is hard-coded, and a project declares nothing to Atlas: no
  `architecture.yml`, no roles. When the picture differs from what the author has in
  mind, the code is what changes. If a view only works because of a special case, it is
  wrong.
- **Atlas is its own first subject.** `atlas view .` must read as the algebra:
  `Disk(repo) → Language.parse → Graph → Flow → View → File(html) → open`. When the
  self-picture shows a box the algebra does not have, the code changes, not the
  picture. `cargo test` pins the self-picture.
- **If the code is good, the picture is good, automatically.** Views are computed
  from types and effects, never from file layout. Hierarchies in the code
  (base classes, composition) are the zoom levels; there is no other source of
  organisation.
- **Keep it simple.** The basic elements first, rigorous and consistent across all
  future codebases, before any richness. One question per feedback round.
- **Plan in Linear first**, project *Atlas — code at the level of types and flows*.
  Record decisions and their reasons on the issue that made them (label *Decision*).

## Practical

- Build and run: `cargo build`, `atlas view .` (install with `cargo install --path cli`;
  the viewer is baked into the binary, so rebuild after editing `viewer/index.html`).
- Version: `version` in the root `Cargo.toml`. Raise the minor number when what Atlas reads
  or draws changes, the patch number for a fix, in the commit that makes the change. The
  build adds the commit it is made from (`0.2.0 (b0ead14)`).
- Tests: `cargo test`. The Rust workspace itself is a test subject.
- `atlas check .` must find nothing on Atlas itself (`engine/tests/principles.rs` pins it).
  A new principle is a rule in `engine/src/principles.rs`, a case in the fixture, and a
  section in `docs/principles.md`; it checks what the graph holds, never a project.
- tree-sitter is pinned to the 0.25 crate line; `tree-sitter-python` 0.25,
  `tree-sitter-rust` 0.24, `tree-sitter-typescript` 0.23.
- Languages: `engine/src/language.rs` is the trait; `python.rs`, `rust.rs` and
  `typescript.rs` implement it. `parts.rs` finds a repository's parts (one per manifest),
  reads each in its language, merges them and joins them where one requests another's
  routes; `atlas view` and the MCP routes use the merged graph, `atlas check` each part. Grammars are reused from tree-sitter crates; what a module writes
  itself is resolution, entry points and boundaries.
- Python's types come from ty (`engine/src/pytypes.rs`): the `ty_*` and `ruff_*` crates are
  pinned to one version together, and need the Rust in `rust-version`. The Python fixture has
  no environment; its libraries are stubs in `engine/tests/fixture/typings`, found through
  `[tool.ty.environment] extra-paths`.
- Run `uv` from the repo root or the scratchpad, never inside `engine/tests/fixture` or a
  project under review: in a directory with a `pyproject.toml`, uv makes a `.venv`, installs the
  project and re-locks `uv.lock`, and ty then reads that `.venv` as the project's environment.
  Pass the project's path as an argument instead.
