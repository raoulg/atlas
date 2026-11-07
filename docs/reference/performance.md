# Atlas's performance, measured every night

Atlas can get slower without anyone seeing it: a reading that regresses, a change that makes a large program take
much more memory. The workflow `.github/workflows/perf.yml` measures the nightly build every night and says when something got
slower. It runs on a schedule and on `workflow_dispatch` only: no push, pull request, commit or landing runs anything
for it. `tools/perf.py` measures and judges; the series is kept on the branch
[`perf-data`](https://github.com/raoulg/atlas/tree/perf-data), whose README draws it.

## What is measured

| measure | how |
| --- | --- |
| `read/<subject>/<cold\|warm>/seconds`, `…/peak_rss` | `atlas-code view --no-open` of a subject. Cold: an empty cache (`XDG_CACHE_HOME`) and no index (`.atlas/index` at the top of the subject's worktree); warm: the same command again. Three rounds a night, the measure their median. Peak memory is the cold reading's; the warm reading's is kept beside it. |
| `read/<subject>/<cold\|warm>/phase/<read\|check\|flow\|page>` | the phases `atlas-code view --timings FILE` reports of the same commands: reading the repository, `atlas-code check`'s principles, `flow::of`, and drawing and writing the page. |
| `read/<subject>/<cold\|warm>/linked/<program\|library>` | how many bodies the reading of the graph linked, the program's and those of the library code it read (its `stats.linked_bodies` and `stats.linked_library_bodies`), as `--timings` reports them; a warm reading takes the graph from the index, and with it the count it was read with. |

The night compiles nothing. It reads with the build of the newest nightly pre-release (`nightly-<stamp>`, which
`nightly.yml` makes an hour before, with the commit it built as its target; `perf.py download --newest-nightly`): what
the nightly channel ships, the open release branch while one is open and `next` otherwise. The series records the
commit measured, whichever branch it came from. A night whose newest build is of the commit the series recorded last
(`next` did not move, or its tests were red and no new build was made) has nothing new to measure: it is skipped, the
summary says so in one line, and the series gets no run. The subjects are the programs `perf` reads
of the corpus (below), each at its fixed revision. Atlas at the commit measured is no subject: it moves with every
commit, so its series would measure the code read together with the reader. All readings use Atlas's defaults
(`--libraries 1`).

Build time and the test suite are not a nightly series: each would take a full compile of Atlas. Nor are the reading
after a file no reader asks about and serve's cold start. The series keeps the nights that measured them, and their
charts end where those nights end. `tools/perf.py` measures each of them by hand: `time` the build, `tests` the test
binaries, `read --kinds unasked serve` the two readings.

| measure | how |
| --- | --- |
| `build/release/seconds`, `build/release/peak_rss` | `cargo build --release --locked -p atlas-code` in a target directory of its own that no cache restores, with no `RUSTC_WRAPPER`: every crate compiles. The dependencies are fetched in a step before it, so the network is not timed. Peak memory is that of the largest process of the build (a `rustc` or the linker). |
| `read/<subject>/unasked/seconds`, `…/peak_rss`, `…/phase/…`, `…/linked/…` | the same command once more, right after the warm one, with a file no reader asks about added to the subject (`docs/x.svg`, taken away after): a reading that rests on what its readers asked of the repository keeps its index; one that rests on the whole listing reads again. |
| `read/<subject>/serve/seconds`, `…/serve/first/seconds`, `…/serve/peak_rss` | `atlas-code serve --no-open` from a cold start (an empty cache and no index), asked `/api/drawn` every 20 ms: the seconds until it names a drawing and draws no more, the complete one, and until it first names one. A build that draws a preliminary picture first names two drawings, and its first comes before its complete one; a build that draws once names one, and the two are the same. Peak memory is the whole session's, the redraw below in it. |
| `read/<subject>/serve/redraw/seconds` | then, in the same session, `docs/x.svg` added: the seconds serve spends drawing again, from the first answer that says it draws to the first that says it is done (at least the 20 ms between two answers); 0 when it starts no drawing within 5 s, which its watch, looking every second, would have. |
| `tests/<package>/<binary>/seconds`, `tests/total/seconds` | each test binary of `cargo test --no-run`, run on its own in its package's directory, as `cargo test` runs it. libtest's own timings (`--format json`, `--report-time`) need nightly Rust, so a binary is timed from outside. Ignored tests and doc tests are not run. |

## The corpus

A benchmark measures the same programs every time, at revisions that do not move, so that a series measures
the reader and not a growing subject. The corpus is one list, `tools/corpus.txt`, that `perf.yml` and `laws.yml`
both read (`tools/perf.py corpus fetch --into DIR --for perf|laws`): a line per program, its name, its
repository, the full commit it is read at, and what reads it: `perf` the nightly series and the weekly run that gives a
release its table (below), `laws` the laws' sample.

```text
atlas-0.70.0    https://github.com/raoulg/atlas          4ebbbae0a877ef3609de15befe577897c3bec69b   perf
goad_toolkit-locked https://github.com/raoulg/goad_toolkit   929d37cc588b27f599b7fbe2e4f5d9733619f897   laws
```

Adding a program is that one line. A name is one revision: a program read at another commit gets another name
(`atlas-0.70.0`, then `atlas-0.75.0`), and so does a program read with another lock (`vectormesh-locked`, read with what its `uv.lock` names), so that its series starts afresh, and `record` refuses a night that reads
a name at another revision than the series has it at. Atlas is in the corpus at the commit of v0.70.0, so
that the reader is measured on Atlas's own code at a subject that does not move. A program is read with its dependencies, as its author has them. Fetching what a lock file names is one
construct in every language, and each committed lock file, at any depth, is installed by its tool in its own
directory: `Cargo.lock` by `cargo fetch --locked` (the crates in Cargo's registry), `uv.lock` by `uv sync --frozen`
(the program's `.venv`, which is the environment ty reads), `package-lock.json` by `npm ci`, `pnpm-lock.yaml` and
`yarn.lock` by pnpm and yarn with their frozen lock (the program's `node_modules`). Scripts of dependencies do not
run. So the corpus measures Python and TypeScript readings that include their libraries, as a user's do, and
without them the reading would depend on what the runner happened to hold. The tools keep their downloads in their
own caches outside the program (`~/.cache/uv`, `~/.npm`, Cargo's home), which the workflows keep between runs for
one list of programs; installing is not timed. A program without a lock file is read as cloned.

A program can be private. Its line says `private` where the repository stands; its repository is its line
`NAME URL` in the secret `CORPUS_PRIVATE`, fetched with the read-only token in the secret `CORPUS_TOKEN`, sent
as a header to its host alone. A run without them fails, as a program that could not be fetched does. A private
program is measured by `perf.yml` only, never sampled by `laws.yml`, whose report names the functions it samples,
and nothing public names it or shows its code: its measures carry its label in the corpus, a reading that fails
says so without Atlas's output, its stages name its libraries and never its own parts or packages, and the release
page knows it by its card alone.

Each subject has a **card**, computed from the reading, never typed: `atlas-code analyze` of the subject after
its readings, with the same build. The card counts the program's own modules and the files they are in, the
lines of those files that are not blank, its functions (methods among them), and of the whole graph, the
libraries' part in it included, the nodes, the edges and the library bodies linked; with its languages and the
revision read. The night keeps the cards with its measures, the series' README shows them, and the release page
names each program by its card.

`--timings FILE` is a hidden option of `atlas-code view`: a JSON object of each phase's seconds, in the order
they ran, and under `linked` the number of bodies the reading of the graph linked (`program`, `library`). The time before the first phase (parsing the command, cloning a URL) is in the wall time only.

Under `stages` it says where the reading's time went (`engine/src/clock.rs`): one row per part, stage, and crate or
package where the stage reads one (`of`, with `library` for a library's), with its seconds and how many times it ran.
Each stage's seconds are its own: a stage within another is not counted in the outer one, so the stages add up to
the time they cover, nearly all of the `read` phase.

| Stage | What it is |
| --- | --- |
| `list`, `index` | the repository's listing and its parts; the index of a part's reading, looked up and kept |
| `parse` | tree-sitter's parse of each file, the program's and the libraries' |
| `items` | the items, `use`s, exports and implementations of the modules |
| `types` | ty, for Python: the environment, and the types of the program and its libraries |
| `reach` | how far into the libraries the program's calls go (`--libraries`): their modules found, followed and read |
| `signatures` | the fields, arms and signatures of the types and functions |
| `scope`, `calls`, `uses`, `close` | a Rust body read: its scope (`Body::new`), its calls in the order they run, what its own code uses, and its compares, edges and touches |
| `body` | a body read, of Python and TypeScript; of Rust, what the four stages above leave of it |
| `callers` | what callers hand a body: paths and statements completed and placed |
| `globals` | the values a module keeps, and the functions that change them |
| `finish`, `fiber` | the graph made from what was read, and the fiber over what the program reaches |

`tools/perf.py` keeps the stages out of the series: `--timings FILE`, given to `read`, `reproduce` or `bisect`, keeps
what Atlas reported of each reading in FILE, and `perf.py stages --timings FILE [--reproduced FILE]` writes it as
tables, the phases, the stages, and the crates and packages by the time reading them took, each the median over
the readings, A and B side by side.

## How a measure is judged

GitHub's hosted runners are shared machines: the same build varies by some percent from night to night,
and the CPU model under `ubuntu-24.04` is not always the same. One night says too little to report on, and
counting nights is guessing; a slowdown is reported when it is reproduced.

- **Baseline**: the median of the measure's last 10 runs, taken from the runs on the same CPU model when
  there are at least 3 of them, else from all. With fewer than 3 runs a measure has no baseline and is
  not judged.
- **Suspect**: a run lies past its baseline by more than the largest of 20 % of the baseline, three
  spreads of the window (its median absolute deviation, scaled to a standard deviation), and a floor of
  0.5 s or 50 MB. Only the measures of the release's table make a night suspect: a reading's cold and warm
  seconds and its cold peak memory. The phases, the bodies linked and the warm peak are kept and shown beside
  them, and say where the time went once a reading is slower. A suspect measure says nothing yet: it is a reason
  to measure again.
- **Reproduced**: in the same run, the last commit on which the measure lay within its baseline (A) and
  the commit measured (B) are measured interleaved, A B A B, five rounds, the readings of one subject taken
  together from one round of readings, as the night takes them, each with the build a release on GitHub holds of
  it (`reproduce --prebuilt`, as the weekly run takes them; A is built on the runner only where the newest seven
  nightlies no longer hold it), on this runner: what the machine does to one it does to the other. The slowdown is reproduced when every
  round of B is slower than every round of A, and the medians differ by more than the floor. Five rounds
  each, all of B past all of A, happens by chance once in 252.
- **Graded by its size**: reproduced below 1.5 times A is *small*: it is commented on the tracking issue
  *Atlas got slower: the nightly measure of next* (label `performance`), with the two commits and the
  A/B numbers, to find out where it came from. From 1.5 times A it is *large*: an issue of its own, titled *Urgent*, names the range from
  A to B and how to bisect it (`mode: bisect`, below), which builds every landing it measures and so runs by hand. Not reproduced,
  it was the runner: a line in the summary, nothing more.

A measure reproduced on one night is known on the next: it is marked in the summary and the charts, and
neither measured again nor reported again until its baseline has taken it in. The run fails only when
it could not measure (a reading that fails, a build that could not be downloaded or made, a subject that could
not be fetched, a series that could not be kept): a regression is a finding, a failure is a gap.

A slower measure is a question, as a finding of `atlas-code check` is: about the change between the two commits,
about the runner, or about the measure.

## Creep, against the last release

A slowdown of a few percent per landing never makes one night suspect. Every Monday the job `weekly`
measures `next` against the last release tag the same way, interleaved on one runner, on the release's table: each
program of the corpus that `perf` reads (`atlas-0.70.0`, Rust; `private-1`, Python and TypeScript;
`vectormesh-locked`, Python), each at its fixed revision, read cold and warm, with the peak memory of the cold
reading, all from one round of readings, and with the cards of `next`'s readings: the programs of the night, three rounds each. The standing goes, one line per
measure, to the issue *Atlas's performance against the last release*. Run by hand (`mode: weekly`) from a branch,
the job takes that branch's `tools/perf.py` and `tools/corpus.txt` and measures `next`, or the ref `to` names
(`origin/release-0.73`, or `release-0.73`, which the job reads as that): a new measure is measured on the runner
before it lands, and a release is measured against the one before.

Nothing is compiled for it. `reproduce --prebuilt` reads each side with the build a release on GitHub holds of its
commit: a release tag `vX.Y.Z` with that release's `atlas-code-x86_64-unknown-linux-gnu.tar.gz`, a commit of a branch
with the nightly pre-release built from it (`nightly-<stamp>`, whose target is that commit). Both are made by the
same `cargo build --release --locked --target …` of the release workflows. A side no release holds, a branch that
moved since its last nightly, is built on the runner as a reproduction builds it, and the output and the standing
say so: one side downloaded and one built here were not made alike.

Three rounds each: all three of B past all three of A happens by chance once in 20, against once in 252 for the
five rounds a suspect night is measured with before it opens an issue. That is enough for the differences a user
notices; a small difference, 10 s against 12 s, is no question for a release, and what grows night by night the
series finds.

## Looking back

Every night is kept with its commit, so where a measure went from 30 s to 45 s over a week, the range is
known. Run the workflow `perf` by hand with `mode: bisect`, `from`, `to` (default `next`) and the measure as
`perf-data` names it, and optionally an `issue` to comment the result on. A range of at most twelve
landings is measured landing by landing, all in each round: one landing that made most of the difference
is named, and where none did, each landing's share is listed. A longer range is halved toward the half that
grew the most. Locally:

```sh
uv run tools/perf.py bisect --repo . --from v0.69.3 --to next --measure read/atlas/cold/seconds --subject atlas=. --out bisect.json
```

Atlas in each commit's own tree grows with the commits, so a bisection of its reading measures the code read
together with the code that reads. To ask one of the two, run `perf` by hand with `mode: compare`: `from` (A),
`to` (B, default the dispatched ref), the measures, separated by spaces, and in `subject` commits of Atlas that both
read, each the subject `atlas-<its short commit>`, its libraries fetched as the night fetches Atlas's. Its measures
name that subject (`read/atlas-4ef2ca0/cold/seconds`), and two commits of Atlas read by both builds are the four
readings that tell a slower reader from a larger subject. The job takes the dispatched ref's `tools/perf.py`,
measures A and B interleaved as `reproduce` does, and writes the comparison with where each reading's time went
(`perf.py stages`) to its summary, to the artifact `perf-compare`, and to the `issue`, when one is given. A bisection
takes `subject` as well, so that a range is bisected for the reader alone (`measure: read/atlas-4ef2ca0/cold/seconds`).

## In every release

Every release's notes carry the same speed point, the same measures on the same programs, release after release,
drawn as bars throughout.

- **One number**: per measure the ratio after / before, and over every measure of the table the geometric mean of
  those ratios, the exponent of the mean log ratio, as SPEC and rustc-perf summarise. It is scale-free (a warm read
  from 0.04 to 0.05 s counts as +25 %, a cold one from 0.69 to 0.68 s as −1.4 %, whatever their seconds) and
  symmetric (twice as fast and twice as slow cancel). A mean of squared differences would lose the sign and be
  ruled by the largest seconds. A measure that falls to 0, a redraw the release no longer starts, has no finite
  log ratio: it stays out of the mean, counted beside it, and its bar runs to the faster edge, said as *to 0*.
- **A line per measure**, beneath it: the measure's mean ratio over the programs as a bar on a log scale from the
  middle, faster to the left and slower to the right, from twice as fast to twice as slow in every release (a ratio
  past it stands at the edge), so that twice as fast and twice as slow are bars of one length; a dot per program
  where its own ratio lies, and beneath the line every program with its change. Whether a change is the release's or
  one program's is read off the dots: all on one side is a shift, one apart is that program. A program's change
  within noise is grey, by the rule of a reproduction made symmetric: it changed when every round of the one release
  lies past every round of the other, on the same side, and the medians lie apart by more than the floor (0.5 s,
  50 MB). A measure taken from two nights has no rounds to judge, and is grey. The floor judges one program; several
  small programs that move alike, each grey, are a shift all the same, and the line shows them so. A measure no
  program changed beyond noise draws no line: those are named in one sentence.
- **The table**, folded: a row per program of the corpus that `perf` reads, named by its card, with its cold read,
  warm read and peak memory (that of the cold read). Each measure is a pair of bars, the release before and this
  one, from the weekly run (the latest scheduled one, or one dispatched with `to=origin/release-X.Y`), both on one
  runner, and else from each release's night. A release does not wait on a run of its own. Two nights ran on two runners, often of other CPUs, and say less than one interleaved run.
  A program that joined the corpus after both releases were measured has no row yet.
- **Every release**, folded: each of those measures on the night of every release since the series began, a bar per
  release. A release's night is the last night that measured a commit of the release and none of the release
  before; a release no night measured between the two has no bar, and borrows none.

```sh
uv run tools/perf.py release-bars --data <a checkout of perf-data> --reproduced weekly.json --run <its url> \
  --repo . --before v0.70.0 --after origin/release-0.71 --version 0.71.0
```

writes `docs/releases/0.71.0/bench.json`, prints the notes' line, `Bench: releases/0.71.0/bench.json | caption`, whose
caption says where each number comes from, and says the summary on its error output; `tools/release_page.py` draws
it. An ad hoc number (a stage that a change made faster, a subject of an issue) stays on its issue and its point.

## The series

`perf-data` is an orphan branch that holds `history.jsonl`, one line per run (its commit, date, run,
runner, every measure with its samples, the cards of the subjects it read, and which measures were suspect and
slower), and `README.md`, which `tools/perf.py record` draws from it: the subjects by their cards, a table per kind
of measure, with a chart of the last 90 runs
for the build, each reading's wall time and memory, the tests' total, and any measure while it is suspect.
GitHub shows that README at the branch; the documentation site is not touched.

## Cost

One `ubuntu-24.04` runner a night: a download, three rounds of a cold and a warm reading of each program `perf`
reads, and their cards. A suspect night adds the reproduction: a second download and five interleaved rounds of the
suspect subject's readings. The job's timeout is 90 minutes, which leaves room to build an A no nightly holds. The
weekly job downloads two builds and measures the table interleaved, three rounds, within 120 minutes, which leaves room
to build a side no release holds. `bisect` and `compare` build the commits they measure; each first removes toolchains
the runner holds and no job uses, so that two builds and their tests fit on its disk.

## Running it by hand

```sh
uv run tools/perf.py corpus fetch --into programs --for perf
uv run tools/perf.py read --atlas target/release/atlas-code --subject pairs=engine/tests/pairs --out run.json
uv run tools/perf.py tests --workspace . --out run.json -- -p atlas-code --test help
uv run tools/perf.py record --data perf-data --run run.json --commit "$(git rev-parse HEAD)"
```

`record` without `--summary` prints the summary. A history made on a laptop says nothing about the
runner's: keep it apart from `perf-data`. `python3 tools/test_perf.py` holds the laws of the corpus, the cards and
the release's table.
