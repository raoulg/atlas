# Atlas's performance, measured every night

Atlas can get slower without anyone seeing it: a build that takes much longer, a reading that regresses,
a test suite that grows. The workflow `.github/workflows/perf.yml` measures `next` every night and says
when something got slower. It runs on a schedule and on `workflow_dispatch` only: no push, pull request,
commit or landing runs anything for it. `tools/perf.py` measures and judges; the series is kept on the
branch [`perf-data`](https://github.com/raoulg/atlas/tree/perf-data), whose README draws it.

## What is measured

| measure | how |
| --- | --- |
| `build/release/seconds`, `build/release/peak_rss` | `cargo build --release --locked -p atlas` in a target directory of its own that no cache restores, with no `RUSTC_WRAPPER`: every crate compiles. The dependencies are fetched in a step before it, so the network is not timed. Peak memory is that of the largest process of the build (a `rustc` or the linker). |
| `read/<subject>/<cold\|warm>/seconds`, `…/peak_rss` | `atlas view --no-open` of a subject. Cold: an empty cache (`XDG_CACHE_HOME`) and no index (`.atlas/index` at the top of the subject's worktree); warm: the same command again. The median of three rounds. |
| `read/<subject>/<cold\|warm>/phase/<read\|check\|flow\|page>` | the phases `atlas view --timings FILE` reports of the same commands: reading the repository, `atlas check`'s principles, `flow::of`, and drawing and writing the page. |
| `tests/<package>/<binary>/seconds`, `tests/total/seconds` | each test binary of `cargo test --no-run`, run on its own in its package's directory, as `cargo test` runs it. libtest's own timings (`--format json`, `--report-time`) need nightly Rust, so a binary is timed from outside. Ignored tests and doc tests are not run. |

The subjects are Atlas itself, at the commit measured, and the public programs `laws.yml` reads, at
the same fixed revisions; a new revision is a change to both workflows, and starts that subject's series
again. All readings use Atlas's defaults (`--libraries 1`).

`--timings FILE` is a hidden option of `atlas view`: a JSON object of each phase's seconds, in the order
they ran. The time before the first phase (parsing the command, cloning a URL) is in the wall time only.

## How a measure is judged

GitHub's hosted runners are shared machines: the same build varies by some percent from night to night,
and the CPU model under `ubuntu-24.04` is not always the same. The judgement is made for that:

- **Baseline**: the median of the measure's last 10 runs, taken from the runs on the same CPU model when
  there are at least 3 of them, else from all. With fewer than 3 runs a measure has no baseline and is
  not judged.
- **Suspect**: a run lies past its baseline by more than the largest of 20 % of the baseline, three
  spreads of the window (its median absolute deviation, scaled to a standard deviation), and a floor of
  0.5 s or 50 MB. The floor keeps a test that takes a tenth of a second from raising anything.
- **Slower**: suspect on this run and on the run before. A single slow night is noise until the next one
  says otherwise; `workflow_dispatch` asks the next one at once.

Slower measures are written at the top of the run's summary, as warnings on the run, and in the tracking
issue *Atlas got slower: the nightly measure of next* (label `performance`): opened when none is open,
commented on when one is. A measure is reported to the issue once, on the run it becomes slower; it
stays marked slower in the summary for as long as it lasts. A measure that stays slower raises its own
baseline after some nights, and is then no longer reported: the issue is where it is answered. The run
itself fails only when it could not measure (a build, a reading or a test that fails, a subject that could
not be fetched, a series that could not be kept): a regression is a finding, a failure is a gap.

A slower measure is a question, as a finding of `atlas check` is: about the change between the two commits
the issue names, about the runner, or about the measure.

## The series

`perf-data` is an orphan branch that holds `history.jsonl`, one line per run (its commit, date, run,
runner, every measure with its samples, and which measures were suspect and slower), and `README.md`,
which `tools/perf.py record` draws from it: a table per kind of measure, with a chart of the last 90 runs
for the build, each reading's wall time and memory, the tests' total, and any measure while it is suspect.
GitHub shows that README at the branch; the documentation site is not touched.

## Cost

One `ubuntu-24.04` runner a night, under 45 minutes: the clean build, three rounds of cold and warm
readings of five subjects, the tests' build (from the cache of this job) and one run of every test binary.
The job's timeout is 75 minutes.

## Running it by hand

```sh
uv run tools/perf.py read --atlas target/release/atlas --subject pairs=engine/tests/pairs --out run.json
uv run tools/perf.py tests --workspace . --out run.json -- -p atlas --test help
uv run tools/perf.py record --data perf-data --run run.json --commit "$(git rev-parse HEAD)"
```

`record` without `--summary` prints the summary. A history made on a laptop says nothing about the
runner's: keep it apart from `perf-data`.
