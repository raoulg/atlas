# Laws review, 2026-10-20: the programs

The review traced every visual principle of Atlas (folding and unfolding, the sides of places, lines) and
every rule of `atlas check` back to `docs/reference/mathematics.md` and `docs/reference/principles.md`, and
tested each with a program of a few lines. This folder holds those programs, so that every card of the review
page can be replayed. The review page is the artifact
[Atlas tegen zijn wetten](https://claude.ai/artifact/KKy88oVK4qbzUVPoWVLBAr); its cards are named Z (folding),
C (check rules), T (two versions), R (the arrows of a route) and M (the checks themselves).

Every observation was made with a release build of Atlas 0.68.0 (`8c5cc49`).

The programs sit under `testdata/` because every manifest below a repository roots a part of it unless a
directory on its path is one Atlas reads as test data (`knowledge::TEST_DIRS`). Under `testdata/`,
`atlas view .` and `atlas check .` on Atlas do not read them as parts of Atlas.

## Replaying a program

```bash
atlas check --json --no-cache <program>
atlas view <program> --no-open --no-cache --out page.html
```

The page opens on a view given in its address: `page.html#v=<the view as JSON, URL-encoded>`, for example
`{"route": "clean.run", "open": {"steps": ["clean.run/1"]}}`. With `window.ATLAS_ORACLE = true` set before the
page loads, the page checks each picture against its plan; `atlasPlan.fine` is the finest plan of the route,
`atlasPlan.coarse` the open picture's plan and `atlasPicture` what was drawn. `testdata/topo/probe.py` does
this for a list of views (Playwright on the installed Chrome) and writes what each view drew to `dump.json`:

```bash
ATLAS=target/release/atlas uv run docs/reviews/2026-10-20-laws/testdata/topo/probe.py <program> <out-dir> '[{"route": "clean.run"}]'
```

The programs of the two-versions cards are git repositories. `testdata/engine-e/` keeps each as its last
commit, and `testdata/engine-e/scripts/` rebuilds each with its history in `$E` (default: `atlas-laws-yoneda`
in the temporary directory):

```bash
E=/tmp/yoneda zsh docs/reviews/2026-10-20-laws/testdata/engine-e/scripts/y1f.sh
atlas view /tmp/yoneda/y1f-upstream-producer-swap --since HEAD~1 --no-open --no-cache --out page.html
atlas changes HEAD~1 --repo /tmp/yoneda/y1f-upstream-producer-swap
```

`shots/` holds the pictures the review page shows, and `shots/views.json` names the program, the view and,
for a two-versions picture, the revision each one was drawn from.

## What each directory holds

### `testdata/topo/`: folding and unfolding (Z1–Z13)

| Program | Cards | What it shows |
| --- | --- | --- |
| `m1-readback-inside` | Z1 | `refresh` writes `cache.json` and reads it back; the read-back line is left out of the plan and the oracle |
| `m2-own-and-reached` | Z2 | `export` writes `out.csv` itself and through `write_header`: one line `×2` instead of two channels |
| `m3-dotted-packages` | Z3, Z4 | packages `app` and `lib`: the dotted line appears only when `app` is opened; one `Rec` becomes two boxes |
| `m4-family-splits-siblings` | Z5, Z6 | opening the family `Shape` splits a merged step; `report(c: Circle, q: Square)` loses `Square` |
| `m5-two-of-one-type` | Z7 | `combine(left, right)`: its inputs fold into `join · +2 types`, its own arrow takes what its body made |
| `m6-own-input-moment` | Z8, Z9 | open `tidy`: the `Table` it makes inside stands outside its frame; closed, one hand-over reads `×2` |
| `m7-folds` | Z11, Z12 | a folded `data/` stands between the steps and reverses their order; `[project.scripts]` in a flat module |
| `m7p-folds-package` | Z12 | the same program as a package: the entry is a CLI |
| `m8-holds` | Z13 | what holds: the twin `⇄`, the coproduct of arms, dispatch |

Z10 (the plan budget) has no program: it is measured on Atlas itself, route `atlas` (`atlas.main`).
`FINDINGS.md` holds the notes written while reviewing; its T1–T12 are cards Z1–Z12, its T13 is card M4 and
its HOLDS card Z13.

### `testdata/check-a/`, `check-b/`, `check-c/`: the rules of `atlas check` (C1–C11)

Each program is named after the rule it tests and the form it tries: `sums-inline-py` is the unnamed union in
Python, `places-env-default-rs` an environment variable with a default in Rust.

| Prefix | Directory | Card |
| --- | --- | --- |
| `sums-` | check-a | C1 |
| `overrides-` | check-a | C2 |
| `doors-` | check-a | C3 |
| `places-` | check-a | C4 |
| `changes-` | check-b | C5 |
| `gives-` | check-b | C6 |
| `choices-`, `unnamed-` | check-b | C7 |
| `nothing-` | check-b, check-c | C8 |
| `layers-` | check-c | C9 |
| `lens-` | check-c | C10 |
| `prophecy-` | check-c | C11 |

The pairs the cards show: C1 `sums-alias-py`, `sums-inline-py`, `sums-ast-py`; C2 `overrides-mro-py`,
`overrides-structural-ts`, `overrides-narrow-param-ts`; C3 `doors-overload-py`, `doors-any-py`, `doors-any-ts`;
C4 `places-direct-literal-py`, `places-caller-literal-py`, `places-default-param-py`, `places-env-default-py`;
C5 `changes-param-name-order-py`, `changes-list-param-py`, `changes-rebind-copy-py`; C6 `gives-unannotated-ts`;
C7 `choices-across-bodies-py`, `choices-derived-local-py`, `unnamed-spelling-py`; C8 `nothing-logging-py`,
`nothing-exit-assert-py`; C9 `layers-py-init-and-child`; C10 `lens-py-sqlite-two-tables`,
`lens-py-putget-broken-passes`; C11 `prophecy-rs-false-prophecy-forms`.

`testdata/tscheck/` holds the two TypeScript overrides of C2 as `tsc` checks them: the compiler accepts both.

### `testdata/engine-d/`: the arrows of a route (R1–R5)

Each program has an `EXPECT.md`: the picture the mathematics asks for, what 0.68.0 draws, and the defect by
letter (A–S), with its cause.

| Prefix | Cards |
| --- | --- |
| `reach-two-callers-one-helper-*`, `reach-callsite-orders-py`, `paths-held-by-object-py` | R1 |
| `reach-ctor-of-exported-class-*` | R2 |
| `paths-*` | R3 |
| `side-*`, `readback-shortcut-private-class-py` | R4 |
| `functor-*` | R5 |
| `own-and-reached-*` | Z2 |
| `side-many-files-one-dir-fold-py`, `side-db-two-tables-py` | Z11 |

### `testdata/engine-e/`: two versions (T1–T7)

| Repository | Cards |
| --- | --- |
| `y1f-upstream-producer-swap`, `y2e-class-rename` | T1 |
| `y2d-unrelated-pair`, `y2c-*`, `y2f-rename-own-named-place`, `y1d-type-gains-field` | T2 |
| `y1g-caller-drops-call`, `y1e-caller-makes-path` | T3 |
| `y4f-family-vs-route-fiber`, `y4l-fold-vs-part-zoom` | T4 |
| `y4p-two-parts-one-package-name` | T5 |
| `y3c-bundle-count` | T6 |
| `y1a-body-only`, `y1b-callee-effect`, `y1c-writer-moves`, `y1h-writer-same-short-name`, `y1k-field-renamed`, `y2a-one-rename`, `y2b-two-renames`, `y3a-package-border`, `y3d-folded-dir-count` | T7 |

`y4l-fold-vs-part-zoom` and `y4p-two-parts-one-package-name` are one commit each and have no script.
`probe-types` is a program without history whose graph (`atlas analyze`) was read for the fields and parameter
types it keeps. `tools/look.py` dumps what a
two-versions page marks; `tools/payload.py` prints the payload `atlas view` bakes into its page.
