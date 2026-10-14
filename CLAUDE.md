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

The rigorous half and the visual half are the same structure. Read `docs/reference/design.md`
§0 before changing either.

## The shared language, in six sentences

1. Objects are places where data rests: Memory(type), File(path), Database(table),
   Network(url), Process(command), Screen(human), Library(what a dependency produces),
   Global(a value a module keeps, which its functions change).
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
  `Disk(repo) → Repo.read → Repo → Language.parse → Graph → Flow → View → File(html) → open`. When the
  self-picture shows a box the algebra does not have, the code changes, not the
  picture. `cargo test` pins the self-picture.
- **If the code is good, the picture is good, automatically.** Views are computed
  from types and effects, never from file layout. Hierarchies in the code
  (base classes, composition) are the zoom levels; there is no other source of
  organisation.
- **Atlas lays problems bare; it does not solve them.** When a subject draws oddly, first ask
  whether the code says what is missing. If it does not, the gap is the finding, drawn as a
  place the code does not name; Atlas never guesses on the code's behalf. A subject is no
  benchmark: settle "Atlas or the code?" with a pair in `engine/tests/pairs`, the same program
  once as good practice writes it and once with the usual shortcut. The clean side must read as
  the algebra says, the shortcut must show as a shortcut (`engine/tests/pairs.rs`).
- **Every change to Atlas moves the pairs.** The mockups in `engine/tests/pairs` (one program
  per language, clean and sloppy) grow with Atlas. For every change to what Atlas reads, draws or
  checks, answer three questions in them before it lands: what does this lay bare, how does the
  perfect code look, how does the sloppy code look? Then show in `pairs_<language>.rs` that Atlas
  tells the two apart. Only what can be traced exists: a difference is a fact in the graph that
  differs. Principles are stated once, on the graph; each language traces what it can, and a
  new language brings a reader, never a principle.
- **Isomorphic is the same.** Forms that say the same thing are treated alike, in every language,
  even where they are not exactly equivalent: typer's parameter default, clap's `default_value` and
  parseArgs' `default:` are each an option the caller can override. Their pushout is the construct
  Atlas draws, and some level of the picture shows it as one thing; zooming in may show the
  language's form beneath it. Before a reader treats a form its own way, ask what it is isomorphic to.
- **The mathematics has one home.** Every notion Atlas uses (a functor, a fiber, Yoneda, a
  quotient) has an entry in `docs/reference/mathematics.md`: what it is, what it is in Atlas, where
  it lives. A notion Atlas starts to use is written there first.
- **Keep it simple.** The basic elements first, rigorous and consistent across all
  future codebases, before any richness. One question per feedback round.
- **Plan in Linear first**, project *Atlas — code at the level of types and flows*.
  Record decisions and their reasons on the issue that made them (label *Decision*).
- **Two agents work in this repository: Claude Code and Codex.** Before starting, claim the
  issue in Linear: set it In Progress and open its description with a *Coordination* section that
  names the active owner, the branch, the worktree, the base commit and the files in scope. Work
  in your own worktree and branch; never edit another agent's worktree, branch or files in scope.
  Claude's checkout is `/Users/rgrouls/code/packages/atlas` on `atlas-obeys-its-algebra`, its
  subagents' are `.claude/worktrees/agent-*`; Codex's are under `~/.codex/worktrees/`. `main` moves
  only when Raoul asks. When work lands, the issue goes to Done with a comment saying where.

## Practical

- Build and run: `cargo build`, `atlas view .` (install with `cargo install --path cli`;
  the viewer is baked into the binary, so rebuild after editing `viewer/index.html`).
- Version: `version` in the root `Cargo.toml`. Raise the minor number when what Atlas reads
  or draws changes, the patch number for a fix, in the commit that makes the change. The
  build adds the commit it is made from (`0.2.0 (b0ead14)`).
- Tests: `cargo test`. The Rust workspace itself is a test subject. CI runs them on Linux, macOS
  and Windows: a test that writes files builds them as the platform lays them out (a virtual
  environment keeps its packages in `Lib\site-packages` on Windows) and compares paths as `Path`s,
  never as strings with `/`.
- Format: `cargo fmt` before every commit (`rustfmt.toml`: 140 columns); CI checks it. It
  formats the workspace only: the fixtures and pairs under `engine/tests` are subjects, written
  as their authors would, and their line numbers are pinned.
- A change that should leave the pictures as they were is shown to with `tools/pictures.py`:
  `uv run tools/pictures.py draw <atlas> <repo> <dir>` with the build before and after, then
  `uv run tools/pictures.py compare <before> <after>` (Playwright on the installed Chrome).
- A viewer change is walked on the subjects before it is committed: `uv run tools/pictures.py
  walk <atlas> <repo> <dir>` opens every package, every route and every family in a route, and
  must report no view that throws, draws nothing, or opens a family into other moments.
- `atlas check .` must find nothing on Atlas itself (`engine/tests/principles.rs` pins it).
  A new principle is a rule in `engine/src/principles.rs`, a case in the fixture, a clean and a
  sloppy case in the pairs, and a section in `docs/reference/principles.md`; it checks what the graph
  holds, never a project.
- tree-sitter is pinned to the 0.25 crate line; `tree-sitter-python` 0.25,
  `tree-sitter-rust` 0.24, `tree-sitter-typescript` 0.23.
- Languages: `engine/src/language.rs` is the trait; `python.rs`, `rust.rs` and
  `typescript.rs` implement it. `parts.rs` finds a repository's parts (one per manifest),
  reads each in its language, merges them and joins them where one requests another's
  routes; `atlas view` and the MCP routes use the merged graph, `atlas check` each part. Grammars
  are reused from tree-sitter crates. A reader supplies its language's syntax and facts:
  resolution, how its names get their types, entry points, which calls cross into the world,
  and syntax tables (`ChoiceSyntax`, `CompareSyntax`, `OwnTypes`). What every language shares is
  written once: a body's steps and the touches its calls make (`body.rs`), carrying paths, URLs
  and statements to the callers and placing them (`paths.rs`), exports and route prefixes (`builder.rs`,
  `entries.rs`), `method_of` and `foreign_type` (`builder.rs`). Each reader's `link_calls` only
  says in which order its steps run.
- `engine/src/repo.rs` is the one place the engine reads a repository: the listing once (with the
  `ignore` crate: no hidden directory, nothing git ignores, no dependency or build directory), a
  file's text once, when a reader first asks. `language::read` is the step that reads: the
  repository, or for a package named on its own the project it is in (`Repo::asked`). Readers,
  parts and the Makefile take the `Repo`, never the disk; a part is a view of the same reading.
  Library code is the environment's: Python's through ty, TypeScript's in `node_modules`, Rust's
  where Cargo keeps it (`--libraries N` rounds). The graph keeps of a library the fiber over what
  the project reaches; what lies beyond it is glued into the library function at its border.
- What Atlas knows of libraries and frameworks is a table in `engine/src/knowledge.rs`, one per
  kind of fact; a reader looks it up and never tests a library's name in its control flow
  (`engine/tests/readers.rs` checks both this and that no project's name stands in the engine).
- Python's types come from ty (`engine/src/pytypes.rs`): the `ty_*` and `ruff_*` crates are
  pinned to one version together, and need the Rust in `rust-version`. The environment is the
  project's own (`[tool.ty.environment]`, else its `.venv`), never `VIRTUAL_ENV` or `PATH`. The Python fixture has
  no environment; its libraries are stubs in `engine/tests/fixture/typings`, found through
  `[tool.ty.environment] extra-paths`.
- Run `uv` from the repo root or the scratchpad, never inside `engine/tests/fixture` or a
  project under review: in a directory with a `pyproject.toml`, uv makes a `.venv`, installs the
  project and re-locks `uv.lock`, and ty then reads that `.venv` as the project's environment.
  Pass the project's path as an argument instead.
- The shell is zsh: a loop over subjects must not name its variable `path` (`while read name path`),
  since zsh ties `path` to `$PATH` and every command in the body is then "not found". `atlas analyze`
  and `atlas view` write into `.atlas/` at the top of the subject's git repository unless given
  `-o`/`--out`; give the scratchpad when the subject is not ours.
