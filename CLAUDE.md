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
- **Atlas is its own first subject.** `atlas-code view .` must read as the algebra:
  `Disk(repo) → Repo.read → Repo → Language.parse → Graph → (Flow, Findings) → Html → File(html) → open`:
  the graph goes to two arrows, `flow::of` and the principles' check, and the page is drawn from the
  pair; the view it opens in is the command line's. The picture is never bent to the line. Where they
  differ, ask which is wrong. A box that comes from how the code happens to be written (three
  `String`s in a tuple, an untyped value, a step that does two jobs) means the code changes. An arrow
  the code really computes and the line does not name means the line changes: the line is derived
  from what Atlas does, never an axiom the code is cut to. The self-picture is looked at, never
  pinned by a test: a test that reads Atlas's own code fails when Atlas is rewritten, not when a law breaks.
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
- **A test holds a law, never an outcome.** Each test names the law, principle, reader fact or
  interface contract it holds, and its subject is made for it: a small program, a pair, a graph
  drawn by `proptest`. Where a law says two routes agree (fibers commute, quotients compose, a
  saturated filter commutes with its quotient, a graph read back from code it was written as), the
  test computes both and compares them; where it says they differ, a counterexample shows it.
  A test that pins what Atlas happens to say (a finding's wording, a route's text, a count of a
  subject's parts) or how a subject happens to be written is no law. A test that changes is read as
  a question, never as an instruction to change the code or the test: which law moved, and is it the
  one this change meant to move?
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
- **A design question gets a derivation, not a vote.** What Atlas draws is settled in this order:
  first the six sentences and `docs/reference/mathematics.md`, as consistently as they allow (forms
  that are isomorphic are the same; a form is never made special when a general rule already covers
  it); where they leave room, UI/UX and good coding practice; only what is left after both is
  Raoul's preference. Write the derivation on the issue (label `decision`) and act on it; a
  principle it adds goes into `mathematics.md`. The map is not the territory: how fine a picture is
  drawn is answered by zoom and filters (quotients and fibers), never by dropping a fact from the
  graph. What does go to Raoul is put visually: what the question means for the picture, the
  pictures it chooses between side by side, with the derivation that narrowed it to them.
- **Keep it simple.** The basic elements first, rigorous and consistent across all
  future codebases, before any richness. One question per feedback round.
- **Plan in GitHub issues first**, in `raoulg/atlas`: Atlas is developed in the open. Record
  decisions and their reasons on the issue that made them (label `decision`). Older commits name
  issues by an id of the form `PTT-…`; an open issue that has one names it on its first line.
- **Two agents work in this repository: Claude Code and Codex.** Before starting, claim the
  issue on GitHub: label it `in progress` and comment a *Coordination* note that names the active
  owner, the branch, the worktree, the base commit and the files in scope. Work in your own
  worktree and branch; never edit another agent's worktree, branch or files in scope. Claude's
  checkout is `/Users/rgrouls/code/packages/atlas`, kept on `next`; its subagents'
  are `.claude/worktrees/agent-*`; Codex's are under `~/.codex/worktrees/`. Since `next` is checked
  out there, a worktree lands its branch from a detached head:
  `git checkout --detach origin/next && git merge --no-ff <branch> && git push origin HEAD:next`. Branches, versions and
  releases follow the next section. When work lands, the issue is closed with a comment saying where:
  the commit and the version it is released as (on `next`, the nightly that has it). A commit names
  the issue it serves (`#19`).

## Branches, versions and releases

Two long-lived branches, one release train, and while a release is finished its release branch. Everything
else is a short-lived topic branch.

| Branch | Holds | Version | Built as | Who moves it |
| --- | --- | --- | --- | --- |
| `next` | integration: everything finished | the next minor, `X.Y.0` | nightly, `X.Y.0-nightly.<timestamp>` | the integrating agent, by merging the topic branches of a round and testing them once |
| `release-X.Y` | the release being finished, cut from `next` | `X.Y.0` | nothing; `ci` runs on it | the agent that prepares the promotion |
| `main` | what is released | the released `X.Y.Z` | stable, tag `vX.Y.Z` | Raoul only |
| `<topic>` | one piece of work | none | nothing | its owner |

1. **Start from `next`**, in your own worktree and branch. A topic branch never raises the version,
   never makes a tag, never merges into `main`, and is removed once merged: its worktree
   (`git worktree remove <path>`, which also frees its `target`, several GB), its local branch and
   its remote branch. Check first that the worktree has no changes and that
   `git rev-list --count origin/next..HEAD` is 0. The agent that merges does this in the same turn.
2. **Land on `next`** when `cargo fmt` and `cargo test` pass: merge the topic branch
   (a merge commit, no squash), push, close the issue. New work, documentation, the release
   workflows and the tap all go to `next`, also while it is only documentation. The landing carries
   the issue's note, `docs/changes/<issue>.md`, with its picture (`docs/changes/README.md`): what a
   user sees differently, drawn with Atlas where Atlas draws it. A release report is put together
   from these notes, never written afresh.
   **The whole suite runs once per landing, never once per branch or per commit.** One agent
   integrates: the session that started the others, or the agent that works alone. A topic's owner
   (a subagent, or a session the integrator started) runs only the test files of what it changes
   (`cargo test -p atlas-engine --test pairs_python`, `--lib` for the unit tests), never the whole
   `cargo test`, never before a commit and never as a last check; it pushes its branch, lands
   nothing, and reports the branch and the tests it ran. The integrator writes this into every brief.
   It waits until every topic of the round has reported, merges the branches into one branch from
   `origin/next`, runs `cargo fmt --check`, `cargo test` (and for a viewer change `walk` and
   `topology`) once on the whole, fixes there what fails, and lands that. A failure there is found by
   testing the branches in it, not by running the suite again for each. When `next` moves before the push, merge
   it again and run again only the suites its new commits touch: documentation and CI touch none.
3. **Cut, then promote**: once what a release carries is on `next`, the agent that prepares it cuts
   `release-X.Y` from `next` (`git push origin <commit>:refs/heads/release-X.Y`), and new work keeps
   landing on `next`. Only the release's own finishing lands on `release-X.Y`: its notes and pictures, and
   a fix of something it carries, landed as on `next` (from a detached `origin/release-X.Y`) and merged
   into `next` in the same turn, so `next` never lacks what the release has. When Raoul says so,
   `release-X.Y` is merged into `main` (a merge commit), `main` is tagged `vX.Y.0` and pushed, and the
   release branch is removed; the tag builds the stable release and, with the secret `TAP_TOKEN`, updates
   the tap. `main` then holds nothing `next` lacks. An agent prepares the promotion (conflicts
   resolved, docs pictures drawn, checks green, the release's notes `docs/releases/<version>.md`
   gathered from `docs/changes/` and made a page by `uv run tools/release_page.py`, which must name no
   point without a picture) and stops before the push. That a point has a picture is the floor: each is
   as visual as 0.68.0's, an `Example:` that both releases draw, a `Compare:` of Atlas's pictures, `Bars:`
   of numbers, or a drawing that carries the point, never a table of text. One point says how fast the
   release is against the one before: `uv run tools/perf.py release-bars` gives its `Bars:` from the
   nightly series (`docs/reference/performance.md`).
   The release machinery (`release.yml`, `install.sh`, `install.ps1`, the tap job, `tools/tap_formula.py`, `tools/tap_cask.py`)
   runs only on a `v*` tag, so a change to it on `next` is not exercised until the first tag after a
   promotion, and a fault in it breaks the formula for every stable user. A promotion that carries
   such a change is reviewed by Raoul before the tag, and after the tag the agent checks that the
   release has its five binaries, that the tap's `atlas-code` and `atlas-studio` name the new version and
   their checksums, and that `brew install raoulg/tap/atlas-code`, `brew install --cask raoulg/tap/atlas-studio`
   and `install.sh` install it.
4. **Stable fix**: a bug in released behaviour is fixed on a branch from `main`, merged into `main`
   when Raoul says so, patch number raised, tagged `vX.Y.Z`; then `main` is merged into `next`, and into a release branch if one is open, at once.
   A fix to code that is not released yet is ordinary work on `next`.
5. **Versions are SemVer and are raised in one place each**: the first change on `next` that alters
   what Atlas reads, draws or checks raises the minor, once; later commits leave it alone. After a cut,
   `next` says the release branch's version until that change raises it past it. A stable fix
   raises the patch. Documentation, CI and the tap raise nothing.
6. **History is never rewritten.** No force-push to `main`, `next` or a release branch, no deleted or moved `v*` tag. A
   mistake is a revert commit.
7. **Before any push or tag, read the branch** (`git branch --show-current`). A push to `main`, a `v*`
   tag, a force-push or a change of the repository's settings needs Raoul's yes in this session,
   for that action; an earlier yes does not carry over.

| Channel | Source | Install |
| --- | --- | --- |
| stable | tags `v*` on `main` | `brew install raoulg/tap/atlas-code`, `install.sh` |
| nightly | `next`, built at 03:30 when it changed and its tests pass on three platforms, or by hand | `brew install raoulg/tap/atlas-code-nightly`, `ATLAS_CHANNEL=nightly install.sh` |

## Practical

- Build and run: `cargo build`, `atlas-code view .` (install with `cargo install --path cli`;
  the viewer, `viewer/index.html` with its model `viewer/plan.js`, is baked into the binary, so rebuild
  after editing either).
- Version: `version` in the root `Cargo.toml`, raised as the section above says. The build adds the
  commit it is made from (`0.2.0 (b0ead14)`), and the nightly build says so too
  (`0.2.0-nightly (b0ead14)`: `ATLAS_CHANNEL=nightly` at build time).
- Verification that takes long: a check that runs longer than a few minutes (a walk or topology run over
  a whole subject, a sweep of hundreds of openings) is discussed with Raoul first, saying what it checks
  and what it cannot prove. A sample of a subject's views finds faults; it proves nothing. Where a
  property should hold for every picture, build the code so that it holds by construction (one
  function that all views use) and test that function, on small graphs, before the plan is laid out;
  keep the sweep for a gate before a promotion, and then agree on it. Show Raoul the result before
  verifying it further.
- The laws on real programs: `engine/tests/sample.rs` asks the laws of views and of a route's pictures
  at sites drawn from a seed, within a budget, of the programs it is pointed at (ignored by `cargo test`):
  `ATLAS_SAMPLE_REPOS=a:b ATLAS_SAMPLE_SECONDS=120 cargo test -p atlas-engine --test sample -- --ignored --nocapture`.
  It prints the seed and writes a report (the build, each program's revision, every site); a site that
  breaks is asked again with the seed and `ATLAS_SAMPLE_N` it names. A short budget is ordinary
  development; a long one, agreed on, is a gate before a promotion. `.github/workflows/laws.yml` asks it
  every night of `next` and of public programs at fixed revisions, with a fresh seed; a red run names
  the seed in its summary. A broken site is a question, about
  the law, the code or the sampler, and gets an issue before anything is changed.
- Tests: `cargo test`. The Rust workspace itself is a test subject. CI runs them on Linux, macOS
  and Windows on every push to `main` and in the nightly, for the commit it builds; a push to `next`
  runs no CI, so a fault on one platform shows as a red nightly the next morning, and is fixed then: a test that writes files builds them as the platform lays them out (a virtual
  environment keeps its packages in `Lib\site-packages` on Windows) and compares paths as `Path`s,
  never as strings with `/`.
- The viewer's model, `viewer/plan.js` (the routes, the finest plan of a route, its quotient and the oracle), is a
  function of the page's data that touches no document: the page inlines it, and `engine/tests/plans.rs` runs it in
  node (node on the `PATH`; CI installs it). There every route of every program under `engine/tests/pairs` and
  `engine/tests/plans` is held, for every set of open steps, to the oracle and to the quotient, and a `.plan` file
  beside a program writes down the plan its code implies (`engine/tests/plans/README.md`). A plan is written from the
  code and `docs/reference/mathematics.md`, never from what Atlas draws; a change to what the plan says lands with the
  plans it moves, and a defect a plan shows is marked `known #N` with its issue, the mark going with the fix.
- Builds share one compile cache: `sccache` (from the Brewfile) with `RUSTC_WRAPPER=sccache` in the
  environment, so a new worktree does not compile the dependencies again.
- Disk: cargo names Atlas's builds by its version and by the hash of its dependencies and toolchain,
  so every raise, dependency update or new Rust leaves a full set of builds behind (they reached
  260 GB). `sh tools/clean-old-builds.sh`, run by hand in your own worktree, removes the builds of a
  toolchain no longer installed and then the least recently used builds until `target` is below
  15 GB (`cargo sweep`, from the Brewfile): an earlier version's builds go, today's and the
  dependencies stay, so the next build stays quick. Run it after a version raise has landed, or when
  `df -h ~` runs low; never while a build or a test runs there, and never in another session's
  worktree. No git hook removes builds: a hook runs while a build is made or used (a sidecar copied,
  a release binary a report needs), and a removed build fails far from its cause. The larger cost is the number of worktrees,
  each with its own `target` of 3 to 18 GB: remove a worktree when its branch is merged (see
  Branches). In an agent's shell `rm` is `rip`: `rm -rf target` moves the files to
  `/tmp/graveyard-$USER` and frees nothing (it reached 201 GB). Take build output away with `cargo clean`,
  `git worktree remove` or `command rm -rf`; emptying the graveyard (`rip --decompose`) is Raoul's.
- Format: `cargo fmt` before every commit (`rustfmt.toml`: 140 columns); CI checks it. It
  formats the workspace only: the fixtures and pairs under `engine/tests` are subjects, written
  as their authors would, and their line numbers are pinned.
- A change that should leave the pictures as they were is shown to with `tools/pictures.py`:
  `uv run tools/pictures.py draw <atlas> <repo> <dir>` with the build before and after, then
  `uv run tools/pictures.py compare <before> <after>` (Playwright on the installed Chrome).
- The pictures the README, the Pages and the guide show are drawn by `tools/docs_pictures.py`,
  which keeps what each one draws in `docs/img/drawn.json`. A commit that raises the version runs it
  (lefthook `pre-commit`, `tools/docs-pictures.sh`) and stages the pictures. When one draws
  otherwise, the commit stops and names it with the text that shows it: reread that text, change
  what no longer holds, and commit again.
- The tour's history (`docs/img/interaction/history.png` and `history-dark.png`) is drawn by
  `tools/interaction_pictures.py` from a real run of `atlas-code serve` on six commits of the sunspots pipeline: `uv run
  tools/interaction_pictures.py target/release/atlas-code`. The pages of the tour's two sessions (pointing, designing ahead) are
  drawn by `tools/session_pictures.py`, which plays each session again with `atlas-code serve` and `atlas-code mcp`: `uv run
  tools/session_pictures.py target/release/atlas-code`; their chats are text in the page. Both draw every picture in the light
  and the dark theme, as `docs_pictures.py` does, and the tour shows the one of its theme. No hook runs them; run them
  with a version raise that changes what those pages draw, and reread the text beside a picture they name.
  `docs/img/interaction/drawn.json` keeps what the history draws.
- A viewer change is walked on the subjects before it is committed: `uv run tools/pictures.py
  walk <atlas> <repo> <dir>` opens every package, every route and every family in a route, and
  must report no view that throws, draws nothing, or opens a family into other moments.
- A viewer change also keeps the topology of zoom: `uv run tools/pictures.py topology <atlas> <repo> <dir>` opens one
  box at a time in each route and must report no box that takes a line away, draws a line the closed box did not
  (`more`: by rule 1 a fault of the closed picture), moves a box into or out of a frame that is not the one opened, or
  does not close to the picture it was (`docs/reference/mathematics.md`, "Refinement keeps what it does not refine").
- `atlas-code check` is a signal to look critically at the code and at what the check claims, never an
  oracle and never a target. Zero findings on Atlas is no goal: never change Atlas's code, or bend a
  rule, to keep `atlas-code check .` quiet. Where a finding is wrong, repair the rule and let Atlas carry
  what the rule then finds. `engine/tests/review.rs` keeps Atlas's findings as a reviewed list,
  `REVIEWED`, each with why it stands; it fails when the findings change, so that each change is
  read, not so that the list shrinks. A finding a change brings is answered there: the code changes
  for a reason of its own, the rule is repaired, or the list says why the finding stands.
  A new principle is a rule in `engine/src/principles.rs`, a case in the fixture, a clean and a
  sloppy case in the pairs, and a section in `docs/reference/principles.md`; it checks what the graph
  holds, never a project.
- tree-sitter is pinned to the 0.25 crate line; `tree-sitter-python` 0.25,
  `tree-sitter-rust` 0.24, `tree-sitter-typescript` 0.23.
- Languages: `engine/src/language.rs` is the trait; `python.rs`, `rust.rs` and
  `typescript.rs` implement it. `parts.rs` finds a repository's parts (one per manifest),
  reads each in its language, merges them and joins them where one requests another's
  routes; `atlas-code view` and the MCP routes use the merged graph, `atlas-code check` each part. Grammars
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
  since zsh ties `path` to `$PATH` and every command in the body is then "not found". `status` is
  read-only in zsh; store a command's exit code in a task-specific variable. `atlas-code analyze`
  and `atlas-code view` write into `.atlas/` at the top of the subject's git repository unless given
  `-o`/`--out`; give the scratchpad when the subject is not ours.
