# Atlas's performance, measured every night

Atlas can get slower without anyone seeing it: a build that takes much longer, a reading that regresses,
a test suite that grows. The workflow `.github/workflows/perf.yml` measures `next` every night and says
when something got slower. It runs on a schedule and on `workflow_dispatch` only: no push, pull request,
commit or landing runs anything for it. `tools/perf.py` measures and judges; the series is kept on the
branch [`perf-data`](https://github.com/raoulg/atlas/tree/perf-data), whose README draws it.

## What is measured

| measure | how |
| --- | --- |
| `build/release/seconds`, `build/release/peak_rss` | `cargo build --release --locked -p atlas-code` in a target directory of its own that no cache restores, with no `RUSTC_WRAPPER`: every crate compiles. The dependencies are fetched in a step before it, so the network is not timed. Peak memory is that of the largest process of the build (a `rustc` or the linker). |
| `read/<subject>/<cold\|warm>/seconds`, `…/peak_rss` | `atlas-code view --no-open` of a subject. Cold: an empty cache (`XDG_CACHE_HOME`) and no index (`.atlas/index` at the top of the subject's worktree); warm: the same command again. One reading a night: the rounds of one run lie within a percent of each other on the runner, and a reading that is off makes the measure suspect, which the reproduction then measures five times. |
| `read/<subject>/<cold\|warm>/phase/<read\|check\|flow\|page>` | the phases `atlas-code view --timings FILE` reports of the same commands: reading the repository, `atlas-code check`'s principles, `flow::of`, and drawing and writing the page. |
| `tests/<package>/<binary>/seconds`, `tests/total/seconds` | each test binary of `cargo test --no-run`, run on its own in its package's directory, as `cargo test` runs it. libtest's own timings (`--format json`, `--report-time`) need nightly Rust, so a binary is timed from outside. Ignored tests and doc tests are not run. |

The subjects are Atlas itself, at the commit measured, and the public programs `laws.yml` reads, at
the same fixed revisions; a new revision is a change to both workflows, and starts that subject's series
again. All readings use Atlas's defaults (`--libraries 1`).

`--timings FILE` is a hidden option of `atlas-code view`: a JSON object of each phase's seconds, in the order
they ran. The time before the first phase (parsing the command, cloning a URL) is in the wall time only.

## How a measure is judged

GitHub's hosted runners are shared machines: the same build varies by some percent from night to night,
and the CPU model under `ubuntu-24.04` is not always the same. One night says too little to report on, and
counting nights is guessing; a slowdown is reported when it is reproduced.

- **Baseline**: the median of the measure's last 10 runs, taken from the runs on the same CPU model when
  there are at least 3 of them, else from all. With fewer than 3 runs a measure has no baseline and is
  not judged.
- **Suspect**: a run lies past its baseline by more than the largest of 20 % of the baseline, three
  spreads of the window (its median absolute deviation, scaled to a standard deviation), and a floor of
  0.5 s or 50 MB. A suspect measure says nothing yet: it is a reason to measure again.
- **Reproduced**: in the same run, the last commit on which the measure lay within its baseline (A) and
  the commit measured (B) are built and measured interleaved, A B A B, five rounds (two for a clean build),
  on this runner: what the machine does to one it does to the other. The slowdown is reproduced when every
  round of B is slower than every round of A, and the medians differ by more than the floor. Five rounds
  each, all of B past all of A, happens by chance once in 252.
- **Graded by its size**: reproduced below 1.5 times A is *small*: it is commented on the tracking issue
  *Atlas got slower: the nightly measure of next* (label `performance`), with the two commits and the
  A/B numbers, to find out where it came from. From 1.5 times A it is *large*: the range from A to B is
  bisected in the same run, and an issue of its own, titled *Urgent*, names the landing. Not reproduced,
  it was the runner: a line in the summary, nothing more.

A measure reproduced on one night is known on the next: it is marked in the summary and the charts, and
neither measured again nor reported again until its baseline has taken it in. The run fails only when
it could not measure (a build, a reading or a test that fails, a subject that could not be fetched, a
series that could not be kept): a regression is a finding, a failure is a gap.

A slower measure is a question, as a finding of `atlas-code check` is: about the change between the two commits,
about the runner, or about the measure.

## Creep, against the last release

A slowdown of a few percent per landing never makes one night suspect. Every Monday the job `weekly`
measures `next` against the last release tag the same way, interleaved on one runner: Atlas reading itself
cold and warm, its peak memory, two of the programs cold, and the clean release build. The standing goes,
one line per measure, to the issue *Atlas's performance against the last release*.

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

## In every release

A release's notes carry the main measures against the release before, from the series, so that a release
cannot get slower without its page saying so:

```sh
uv run tools/perf.py release-bars --data <a checkout of perf-data> --repo . --before v0.69.3 --after next
```

prints the `Bars:` line for the release's point on performance (`tools/release_page.py` draws it).

## The series

`perf-data` is an orphan branch that holds `history.jsonl`, one line per run (its commit, date, run,
runner, every measure with its samples, and which measures were suspect and slower), and `README.md`,
which `tools/perf.py record` draws from it: a table per kind of measure, with a chart of the last 90 runs
for the build, each reading's wall time and memory, the tests' total, and any measure while it is suspect.
GitHub shows that README at the branch; the documentation site is not touched.

## Cost

One `ubuntu-24.04` runner a night, about 25 minutes when nothing is suspect: the clean build, one cold and
one warm reading of each of five subjects, the tests' build (from the cache of this job) and one run of
every test binary. A suspect night adds the reproduction (two builds and five interleaved rounds of what is
suspect) and, for a large slowdown, the bisection; the job's timeout is 150 minutes. The weekly job builds
two commits and measures six things interleaved, within the same timeout.

## Running it by hand

```sh
uv run tools/perf.py read --atlas target/release/atlas-code --subject pairs=engine/tests/pairs --out run.json
uv run tools/perf.py tests --workspace . --out run.json -- -p atlas-code --test help
uv run tools/perf.py record --data perf-data --run run.json --commit "$(git rev-parse HEAD)"
```

`record` without `--summary` prints the summary. A history made on a laptop says nothing about the
runner's: keep it apart from `perf-data`.
