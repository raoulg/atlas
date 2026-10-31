# /// script
# requires-python = ">=3.11"
# ///
"""How fast Atlas builds, reads and tests, kept as a series per measure, and a signal when one gets slower.

    uv run tools/perf.py time NAME --out RUN -- COMMAND...
    uv run tools/perf.py read --atlas ATLAS --subject NAME=PATH ... [--rounds N] --out RUN
    uv run tools/perf.py tests --workspace DIR --out RUN [-- CARGO_TEST_ARGS...]
    uv run tools/perf.py record --data DIR --run RUN --commit SHA [--url URL] [--summary FILE] [--suspects FILE]
    uv run tools/perf.py reproduce --repo DIR --a SHA --b SHA (--suspects FILE | --measure M ...) [--subject NAME=PATH ...] --out FILE
    uv run tools/perf.py conclude --data DIR --reproduced FILE [--summary FILE] [--alert FILE] [--standing FILE]
    uv run tools/perf.py bisect --repo DIR --from SHA --to SHA --measure M [--subject NAME=PATH ...] --out FILE
    uv run tools/perf.py release-bars --data DIR --repo DIR --before REF --after REF

`time`, `read` and `tests` measure and add their measures to the run file RUN (JSON):

- `time` runs a command once: its wall time and the peak resident memory of its largest process.
- `read` draws each subject with `atlas-code view --timings`, cold (an empty cache and no index) and warm (the
  same again), once a night (`--rounds` more for a median): the wall time, peak memory and the phases Atlas reports (read,
  check, flow, page), with how many bodies the reading linked, the program's and the libraries'.
- `tests` builds the test binaries once (`cargo test --no-run`), then runs each on its own as `cargo test`
  would, and times it: libtest's own timings are not stable Rust.

`record` holds a run to the history in DIR (`history.jsonl`, one line per run with its commit), adds it,
and draws DIR/README.md with its charts. A measure is suspect when it lies past its baseline, the median
of its last runs (those on the same CPU where there are enough), by more than the larger of a share of
the baseline, three spreads of the window and a floor. A suspect measure says nothing yet: `--suspects`
names each with the last commit that measured it within its baseline, for `reproduce`.

`reproduce` builds two commits and measures them interleaved (A B A B ...) on this machine, so that what
the machine does to one it does to the other: a measure is reproduced when every round of B is slower
than every round of A by more than the floor. Its size grades it: small below LARGE times A, large from
there; a large one is bisected on the spot, so that it names its commit. `conclude` writes the grades
into the history's last run, the summary and the text of the tracking issue (`--alert`, only for what was
reproduced), or the standing of `next` against a release (`--standing`). `bisect` finds the landing in a
range that made a measure slower, or gives each landing's share where no single one did. `release-bars`
gives a release's notes their `Bars:` line from the series. Any failure to measure ends with an error.
"""

from __future__ import annotations

import argparse
import contextlib
import datetime
import html
import json
import os
import platform
import shutil
import statistics
import subprocess
import sys
import tempfile
import tomllib
import time
from dataclasses import dataclass
from pathlib import Path

WINDOW = 10  # runs in a baseline
LEAST = 3  # runs a baseline needs
SHARE = 0.20  # a measure is suspect past its baseline by more than this share of it...
SPREADS = 3.0  # ...and more than this many spreads (MAD scaled to a standard deviation) of the window...
FLOOR = {"s": 0.5, "MB": 50.0}  # ...and more than this
CHARTED = 90  # runs a chart shows
ROUNDS = 5  # interleaved rounds of a reproduction: all five of B past all five of A is 1 in 252 by chance
BUILD_ROUNDS = 2  # a clean build is long: two rounds each
LARGE = 1.5  # a reproduced measure this many times A is large: bisected on the spot, marked urgent
PER_LANDING = 12  # a range of at most this many landings is measured landing by landing; a longer one is halved


# measuring


def measured(
    command: list[str], *, cwd: Path | None = None, env: dict[str, str] | None = None, log: Path | None = None
) -> tuple[float, float]:
    """Run `command` to its end: its wall time in seconds and the peak resident memory of its largest
    process in MB. Its output goes to `log`, else to this script's; a command that fails raises."""
    with open(log, "w") if log else contextlib.nullcontext() as out:
        start = time.perf_counter()
        process = subprocess.Popen(command, cwd=cwd, env=env, stdout=out, stderr=subprocess.STDOUT if out else None)
        # wait4, not wait: the child's own resource use, its peak memory among it
        _, status, usage = os.wait4(process.pid, 0)
        elapsed = time.perf_counter() - start
    if code := os.waitstatus_to_exitcode(status):
        tail = log.read_text(errors="replace")[-4000:] if log else ""
        raise SystemExit(f"{' '.join(command)} failed with {code}\n{tail}")
    # Linux counts the peak in KiB, macOS in bytes
    peak = usage.ru_maxrss / (1024 if sys.platform.startswith("linux") else 1024 * 1024)
    return elapsed, peak


def runner() -> dict:
    cpu = platform.processor() or platform.machine()
    if sys.platform.startswith("linux"):
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                cpu = line.split(":", 1)[1].strip()
                break
    elif sys.platform == "darwin":
        cpu = (
            subprocess.run(["sysctl", "-n", "machdep.cpu.brand_string"], capture_output=True, text=True, check=False).stdout.strip() or cpu
        )
    return {"os": f"{platform.system()} {platform.release()}", "cpu": cpu, "cores": os.cpu_count()}


def add(run: Path, metrics: dict[str, dict]) -> None:
    """Add `metrics` to the run file `run`, made when missing."""
    data = json.loads(run.read_text()) if run.exists() else {"runner": runner(), "metrics": {}}
    data["metrics"].update(metrics)
    run.write_text(json.dumps(data, indent=1, sort_keys=True))


def metric(samples: list[float], unit: str) -> dict:
    return {"value": round(statistics.median(samples), 4), "unit": unit, "samples": [round(s, 4) for s in samples]}


def command_time(args: argparse.Namespace) -> None:
    if not args.command:
        raise SystemExit("time: give the command after --")
    seconds, peak = measured(args.command)
    add(args.out, {f"{args.name}/seconds": metric([seconds], "s"), f"{args.name}/peak_rss": metric([peak], "MB")})
    print(f"{args.name}: {seconds:.1f} s, peak {peak:.0f} MB")


def worktree_top(subject: Path) -> Path:
    """Where Atlas keeps a subject's index: `.atlas/` at the nearest ancestor that holds `.git`."""
    return next((a for a in [subject, *subject.parents] if (a / ".git").exists()), subject)


def read_round(atlas: Path, name: str, subject: Path, libraries: int, scratch: Path, cache: Path) -> dict[str, dict[str, float]]:
    """One cold and one warm `atlas-code view` of `subject`: per kind, its seconds, peak memory, phases and bodies linked."""
    index = worktree_top(subject) / ".atlas" / "index"
    shutil.rmtree(index, ignore_errors=True)
    shutil.rmtree(cache, ignore_errors=True)
    got: dict[str, dict[str, float]] = {}
    for kind in ("cold", "warm"):
        timings = scratch / "timings.json"
        timings.unlink(missing_ok=True)
        env = {**os.environ, "XDG_CACHE_HOME": str(cache), "ATLAS_NO_UPDATE_CHECK": "1"}
        command = [str(atlas), "--libraries", str(libraries), "view", "--no-open"]
        command += ["--out", str(scratch / "page.html"), "--timings", str(timings), str(subject)]
        seconds, peak = measured(command, env=env, log=scratch / "log.txt")
        if not timings.exists():
            raise SystemExit(f"{atlas} wrote no --timings for {name}")
        if kind == "cold" and not any(index.rglob("*")):
            raise SystemExit(f"the cold reading of {name} made no index at {index}: a warm one would measure nothing")
        reported = json.loads(timings.read_text())
        linked = reported.pop("linked", {})
        got[kind] = {
            "seconds": seconds,
            "peak_rss": peak,
            **{f"phase/{p}": t for p, t in reported.items()},
            **{f"linked/{side}": float(n) for side, n in linked.items()},
        }
    return got


def subjects(given: list[str]) -> list[tuple[str, Path]]:
    out = []
    for g in given:
        name, _, path = g.partition("=")
        if not path:
            raise SystemExit(f"--subject {g}: write NAME=PATH")
        out.append((name, Path(path)))
    return out


def command_read(args: argparse.Namespace) -> None:
    atlas = args.atlas.resolve(strict=True)
    readings: dict[str, dict] = {}
    with tempfile.TemporaryDirectory(prefix="atlas-perf-") as scratch:
        scratch = Path(scratch)
        for name, path in subjects(args.subject):
            subject = path.resolve(strict=True)
            samples: dict[str, dict[str, list[float]]] = {"cold": {}, "warm": {}}
            for round_ in range(args.rounds):
                for kind, got in read_round(atlas, name, subject, args.libraries, scratch, scratch / f"cache-{name}").items():
                    for measure, value in got.items():
                        samples[kind].setdefault(measure, []).append(value)
                    print(f"{name} {kind} {round_ + 1}/{args.rounds}: {got['seconds']:.2f} s, peak {got['peak_rss']:.0f} MB")
            for kind, got in samples.items():
                for measure, values in got.items():
                    readings[f"read/{name}/{kind}/{measure}"] = metric(values, unit_of(measure))
    add(args.out, readings)


def test_binaries(workspace: Path, cargo: list[str], env: dict[str, str] | None = None) -> list[tuple[str, Path, Path]]:
    """Each test binary `cargo test --no-run` builds: its name, its executable, its package's directory."""
    build = subprocess.run(
        ["cargo", "test", "--locked", "--no-run", "--message-format=json", *cargo],
        cwd=workspace,
        env=env,
        stdout=subprocess.PIPE,
        text=True,
        check=False,
    )
    if build.returncode:
        raise SystemExit(f"cargo test --no-run failed with {build.returncode} in {workspace}")
    binaries = []
    for line in build.stdout.splitlines():
        message = json.loads(line) if line.startswith("{") else {}
        if message.get("reason") == "compiler-artifact" and message.get("profile", {}).get("test") and message.get("executable"):
            package = message["package_id"].rsplit("#", 1)[-1].split("@")[0]
            target = message["target"]
            name = "lib" if "lib" in target["kind"] else target["name"]
            binaries.append((f"{package}/{name}", Path(message["executable"]), Path(message["manifest_path"]).parent))
    if not binaries:
        raise SystemExit("cargo test --no-run named no test binary")
    return sorted(binaries)


def test_run(name: str, executable: Path, manifest_dir: Path, log: Path) -> float:
    """One test binary, as `cargo test` runs it: in its package's directory, with the package's variables."""
    env = {**os.environ, "CARGO_MANIFEST_DIR": str(manifest_dir), "CARGO_PKG_NAME": name.split("/")[0]}
    seconds, _ = measured([str(executable)], cwd=manifest_dir, env=env, log=log)
    return seconds


def command_tests(args: argparse.Namespace) -> None:
    binaries = test_binaries(args.workspace.resolve(strict=True), args.cargo)
    measures, total = {}, 0.0
    with tempfile.TemporaryDirectory(prefix="atlas-perf-tests-") as scratch:
        for name, executable, manifest_dir in binaries:
            seconds = test_run(name, executable, manifest_dir, Path(scratch) / "log.txt")
            measures[f"tests/{name}/seconds"] = metric([seconds], "s")
            total += seconds
            print(f"{name}: {seconds:.2f} s")
    measures["tests/total/seconds"] = metric([total], "s")
    add(args.out, measures)


# judging


@dataclass
class Verdict:
    measure: str
    value: float
    unit: str
    baseline: float | None = None
    threshold: float | None = None
    runs: int = 0
    suspect: bool = False

    @property
    def change(self) -> float | None:
        return None if not self.baseline else self.value / self.baseline - 1


def history(data: Path) -> list[dict]:
    file = data / "history.jsonl"
    return [json.loads(line) for line in file.read_text().splitlines() if line.strip()] if file.exists() else []


def keep(data: Path, runs: list[dict]) -> None:
    (data / "history.jsonl").write_text("".join(json.dumps(r, sort_keys=True) + "\n" for r in runs))


def judged(run: dict, past: list[dict]) -> list[Verdict]:
    """Each measure of `run` held to its baseline in `past`, the runs before it, oldest first."""
    cpu = run["runner"].get("cpu")
    verdicts = []
    for measure, m in sorted(run["metrics"].items()):
        had = [r for r in past if measure in r["metrics"]]
        alike = [r for r in had if r["runner"].get("cpu") == cpu]
        window = (alike if len(alike) >= LEAST else had)[-WINDOW:]
        verdict = Verdict(measure, m["value"], m["unit"], runs=len(window))
        if len(window) >= LEAST:
            values = [r["metrics"][measure]["value"] for r in window]
            verdict.baseline = statistics.median(values)
            spread = 1.4826 * statistics.median(abs(v - verdict.baseline) for v in values)
            verdict.threshold = max(SHARE * verdict.baseline, SPREADS * spread, FLOOR.get(m["unit"], 0.0))
            verdict.suspect = verdict.value - verdict.baseline > verdict.threshold
        verdicts.append(verdict)
    return verdicts


def amount(value: float | None, unit: str) -> str:
    if value is None:
        return "–"
    return f"{value:.0f} MB" if unit == "MB" else f"{value:.2f} s" if value < 100 else f"{value:.0f} s"


def percent(change: float | None) -> str:
    return "–" if change is None else f"{change:+.0%}"


def table(verdicts: list[Verdict]) -> list[str]:
    rows = ["| measure | this run | baseline | change | past by more than | |", "| --- | ---: | ---: | ---: | ---: | --- |"]
    for v in verdicts:
        mark = "suspect" if v.suspect else ""
        if v.baseline is None:
            mark = f"no baseline ({v.runs} of {LEAST} runs)"
        rows.append(
            f"| `{v.measure}` | {amount(v.value, v.unit)} | {amount(v.baseline, v.unit)} | {percent(v.change)} | {amount(v.threshold, v.unit)} | {mark} |"
        )
    return rows


def summary(run: dict, verdicts: list[Verdict], commit: str, data_url: str | None, known: list[str]) -> str:
    known_ = [v for v in verdicts if v.measure in known]
    suspect = [v for v in verdicts if v.suspect and v.measure not in known]
    judged_ = [v for v in verdicts if v.baseline is not None]
    lines = [f"### Performance of `{commit[:7]}`", ""]
    if suspect:
        lines += [
            f"**Suspect**: {len(suspect)} measure(s) lie past their baseline; they are measured again against the last",
            "commit that lay within it, on this runner, before anything is said.",
            "",
            *table(suspect),
            "",
        ]
    if known_:
        lines += [f"**Still slower**: {len(known_)} measure(s) reproduced on an earlier run, already reported.", "", *table(known_), ""]
    if not suspect and not known_:
        lines += [f"Nothing suspect: {len(judged_)} of {len(verdicts)} measures have a baseline, and each lies within it.", ""]
    r = run["runner"]
    lines.append(f"Runner: {r['cpu']}, {r['cores']} cores, {r['os']}.")
    if data_url:
        lines.append(f"History and charts: {data_url}")
    lines += ["", "<details><summary>Every measure</summary>", "", *table(verdicts), "", "</details>", ""]
    return "\n".join(lines)


def last_within(measures: list[str], past: list[dict]) -> str | None:
    """The latest commit of `past` on which none of `measures` was suspect: the A of a reproduction."""
    for r in reversed(past):
        if not set(measures) & set(r.get("suspect", [])):
            return r["commit"]
    return None


# drawing


def chart(points: list[tuple[str, float, str]], baseline: float | None, unit: str) -> str:
    """A small line chart of a measure's runs, oldest left: a reproduced run red, a suspect one amber, the
    baseline dashed."""
    width, height, pad, left = 360, 96, 10, 54
    values = [v for _, v, _ in points] + ([baseline] if baseline is not None else [])
    low, high = 0.0, max(values) * 1.08 or 1.0  # from zero, as a duration or a size is read

    def x(i: int) -> float:
        return left + (width - left - pad) * (i / max(len(points) - 1, 1))

    def y(v: float) -> float:
        return height - pad - (height - 2 * pad) * (v - low) / (high - low)

    line = " ".join(f"{x(i):.1f},{y(v):.1f}" for i, (_, v, _) in enumerate(points))
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" font-family="sans-serif" font-size="10">',
        f'<text x="{left - 6}" y="{pad + 4}" text-anchor="end" fill="#888">{html.escape(amount(high, unit))}</text>',
        f'<text x="{left - 6}" y="{height - pad}" text-anchor="end" fill="#888">{html.escape(amount(low, unit))}</text>',
        f'<line x1="{left}" y1="{pad}" x2="{left}" y2="{height - pad}" stroke="#888" stroke-width="0.5"/>',
    ]
    if baseline is not None:
        parts.append(
            f'<line x1="{left}" y1="{y(baseline):.1f}" x2="{width - pad}" y2="{y(baseline):.1f}" stroke="#888" stroke-dasharray="3 3"/>'
        )
    parts.append(f'<polyline points="{line}" fill="none" stroke="#1b8a9a" stroke-width="1.5"/>')
    for i, (commit, v, state) in enumerate(points):
        colour = {"reproduced": "#d1495b", "suspect": "#d9922e"}.get(state, "#1b8a9a")
        radius = 3 if state or i == len(points) - 1 else 1.5
        parts.append(
            f'<circle cx="{x(i):.1f}" cy="{y(v):.1f}" r="{radius}" fill="{colour}"><title>{commit[:7]}: {html.escape(amount(v, unit))}{" " + state if state else ""}</title></circle>'
        )
    parts.append("</svg>")
    return "\n".join(parts)


def charted(measure: str) -> bool:
    """The headline measures, each drawn on every run; the others only while suspect."""
    return (
        measure.startswith("build/")
        or measure == "tests/total/seconds"
        or (measure.startswith("read/") and measure.endswith(("/seconds", "/peak_rss")))
    )


def state(run: dict, measure: str) -> str:
    return "reproduced" if measure in run.get("reproduced", []) else "suspect" if measure in run.get("suspect", []) else ""


def readme(runs: list[dict], verdicts: list[Verdict], data: Path) -> str:
    last = runs[-1]
    charts = data / "charts"
    shutil.rmtree(charts, ignore_errors=True)
    charts.mkdir()
    groups: dict[str, list[Verdict]] = {}
    for v in verdicts:
        groups.setdefault({"build": "The release build", "read": "Readings", "tests": "Tests"}[v.measure.split("/")[0]], []).append(v)
    r = last["runner"]
    lines = [
        "# Atlas's performance, measured every night on `next`",
        "",
        "Written by `tools/perf.py` from `history.jsonl`, one line per run; how it measures and judges is in",
        "`docs/reference/performance.md`. An amber dot is a run past its baseline (dashed), measured again on its",
        "runner against the last commit within it; a red dot is one that measurement reproduced.",
        "",
        f"Last run: {last['date']}, `{last['commit'][:7]}`, {r['cpu']}, {r['cores']} cores. Runs kept: {len(runs)}.",
    ]
    for title, vs in groups.items():
        lines += ["", f"## {title}", "", "| measure | last | baseline | change | runs |", "| --- | ---: | ---: | ---: | --- |"]
        for v in vs:
            cell = ""
            now = state(last, v.measure)
            if charted(v.measure) or now:
                points = [(r["commit"], r["metrics"][v.measure]["value"], state(r, v.measure)) for r in runs if v.measure in r["metrics"]]
                name = v.measure.replace("/", "-") + ".svg"
                (charts / name).write_text(chart(points[-CHARTED:], v.baseline, v.unit))
                cell = f"![{v.measure}](charts/{name})"
            mark = f" **{now}**" if now == "reproduced" else f" {now}" if now else ""
            lines.append(
                f"| `{v.measure}`{mark} | {amount(v.value, v.unit)} | {amount(v.baseline, v.unit)} | {percent(v.change)} | {cell} |"
            )
    return "\n".join(lines) + "\n"


def command_record(args: argparse.Namespace) -> None:
    run = json.loads(args.run.read_text())
    if not run.get("metrics"):
        raise SystemExit(f"{args.run} holds no measure")
    args.data.mkdir(parents=True, exist_ok=True)
    past = history(args.data)
    verdicts = judged(run, past)
    suspect = [v.measure for v in verdicts if v.suspect]
    # a measure reproduced slower on the run before is known: it stays marked, and is neither measured again
    # nor reported again, until its baseline has taken it in
    known = [m for m in suspect if past and m in past[-1].get("reproduced", [])]
    entry = {
        "commit": args.commit,
        "date": datetime.datetime.now(datetime.UTC).strftime("%Y-%m-%dT%H:%MZ"),
        "url": args.url,
        "runner": run["runner"],
        "metrics": run["metrics"],
        "suspect": suspect,
        "reproduced": known,
        "large": [m for m in known if m in past[-1].get("large", [])],
    }
    suspect = [m for m in suspect if m not in known]
    keep(args.data, [*past, entry])
    (args.data / "README.md").write_text(readme([*past, entry], verdicts, args.data))
    text = summary(run, verdicts, args.commit, args.data_url, known)
    if args.summary:
        with open(args.summary, "a") as file:
            file.write(text + "\n")
    else:
        print(text)
    a = last_within(suspect, past) if suspect else None
    if args.suspects:
        args.suspects.write_text(json.dumps({"a": a, "b": args.commit, "measures": suspect} if suspect and a else {}, indent=1))
    output(suspect="true" if suspect and a else "false", a=a or "")


def output(**values: str) -> None:
    if file := os.environ.get("GITHUB_OUTPUT"):
        with open(file, "a") as f:
            f.writelines(f"{k}={v}\n" for k, v in values.items())


# reproducing


def git(repo: Path, *args: str) -> str:
    done = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True, check=False)
    if done.returncode:
        raise SystemExit(f"git {' '.join(args)} failed: {done.stderr.strip()}")
    return done.stdout.strip()


@dataclass
class Side:
    """A commit made ready to measure: its worktree and its `atlas`."""

    commit: str
    tree: Path
    atlas: Path


def named(tree: Path) -> tuple[str, str]:
    """The command line's package and binary as the commit names them (`atlas-code`, earlier `atlas`): a
    range measured across a rename builds each commit by its own names."""
    manifest = tree / "cli" / "Cargo.toml"
    if not manifest.exists():
        return "atlas-code", "atlas"
    data = tomllib.loads(manifest.read_text())
    package = data["package"]["name"]
    return package, next((b["name"] for b in data.get("bin", [])), package)


class Bench:
    """Commits of `repo` checked out and built in a scratch directory. One target directory serves every
    commit, so that a commit's build is incremental on the one before; a clean build, when it is the
    measure, gets a target directory of its own. `build` names how a commit is built, `{tree}` and
    `{target}` filled in, and `atlas` where its binary is then."""

    def __init__(self, repo: Path, scratch: Path, build: str, subjects: list[tuple[str, Path]], libraries: int):
        self.repo, self.scratch, self.build, self.libraries = repo, scratch, build, libraries
        self.subjects = dict(subjects)
        self.sides: dict[str, Side] = {}
        self.tests: dict[str, dict[str, tuple[Path, Path]]] = {}

    def side(self, commit: str) -> Side:
        commit = git(self.repo, "rev-parse", commit)
        if commit not in self.sides:
            tree = self.scratch / f"tree-{commit[:12]}"
            git(self.repo, "worktree", "add", "--detach", "--force", str(tree), commit)
            target = self.scratch / "target"
            self.built(tree, target, self.scratch / f"build-{commit[:12]}.log")
            atlas = self.scratch / f"atlas-{commit[:12]}"
            shutil.copy2(target / "release" / named(tree)[1], atlas)
            self.sides[commit] = Side(commit, tree, atlas)
            print(f"ready: {commit[:7]}")
        return self.sides[commit]

    def built(self, tree: Path, target: Path, log: Path) -> tuple[float, float]:
        command = ["sh", "-c", self.build.format(tree=tree, target=target, package=named(tree)[0])]
        env = {k: v for k, v in os.environ.items() if k != "RUSTC_WRAPPER"}
        return measured(command, cwd=tree, env={**env, "CARGO_TARGET_DIR": str(target)}, log=log)

    def subject(self, side: Side, name: str) -> Path:
        """A subject given by a relative path lies in each commit's own tree (Atlas reading itself, as the
        night measured it); an absolute one is the same for both."""
        if name not in self.subjects:
            raise SystemExit(f"no --subject {name}=PATH for a reading of {name}")
        path = self.subjects[name]
        return (path if path.is_absolute() else side.tree / path).resolve(strict=True)

    def test_binaries(self, side: Side) -> dict[str, tuple[Path, Path]]:
        if side.commit not in self.tests:
            keep_dir = self.scratch / f"tests-{side.commit[:12]}"
            keep_dir.mkdir(exist_ok=True)
            env = {**os.environ, "CARGO_TARGET_DIR": str(self.scratch / "target-tests")}
            kept = {}
            for name, executable, manifest in test_binaries(side.tree, [], env):
                copy = keep_dir / name.replace("/", "-")
                shutil.copy2(executable, copy)
                kept[name] = (copy, manifest)
            self.tests[side.commit] = kept
        return self.tests[side.commit]

    def measure(self, side: Side, measure: str) -> float:
        """One sample of `measure` on `side`."""
        parts = measure.split("/")
        with tempfile.TemporaryDirectory(dir=self.scratch) as scratch:
            scratch = Path(scratch)
            if parts[0] == "read":
                _, name, kind, *rest = parts
                got = read_round(side.atlas, name, self.subject(side, name), self.libraries, scratch, scratch / "cache")
                return got[kind]["/".join(rest)]
            if parts[0] == "build":
                seconds, peak = self.built(side.tree, scratch / "clean", scratch / "build.log")
                return peak if parts[-1] == "peak_rss" else seconds
            if parts[0] == "tests":
                binaries = self.test_binaries(side)
                wanted = binaries if parts[1] == "total" else {"/".join(parts[1:3]): binaries["/".join(parts[1:3])]}
                return sum(test_run(n, e, m, scratch / "log.txt") for n, (e, m) in wanted.items())
        raise SystemExit(f"no way to measure {measure}")

    def interleaved(self, commits: list[str], measure: str, rounds: int) -> list[list[float]]:
        """`rounds` samples of `measure` on each commit, the order turned every round so that a machine
        that slows down over the run slows each commit alike."""
        sides = [self.side(c) for c in commits]
        samples: list[list[float]] = [[] for _ in sides]
        for r in range(rounds):
            order = list(range(len(sides)))
            order = order[r % len(order) :] + order[: r % len(order)]
            for i in order:
                samples[i].append(self.measure(sides[i], measure))
            print(f"{measure} round {r + 1}/{rounds}: " + ", ".join(f"{s.commit[:7]} {samples[i][-1]:.2f}" for i, s in enumerate(sides)))
        return samples

    def close(self) -> None:
        for side in self.sides.values():
            subprocess.run(["git", "-C", str(self.repo), "worktree", "remove", "--force", str(side.tree)], check=False)


def unit_of(measure: str) -> str:
    return "MB" if measure.endswith("peak_rss") else "bodies" if "/linked/" in f"/{measure}" else "s"


def compared(measure: str, a: list[float], b: list[float]) -> dict:
    """A against B: reproduced when every sample of B lies past every sample of A, by more than the floor;
    large from LARGE times A."""
    unit = unit_of(measure)
    ma, mb = statistics.median(a), statistics.median(b)
    reproduced = min(b) > max(a) and mb - ma > FLOOR.get(unit, 0.0)
    ratio = mb / ma if ma else None
    grade = ("large" if ratio and ratio >= LARGE else "small") if reproduced else "noise"
    return {
        "unit": unit,
        "a": [round(x, 4) for x in a],
        "b": [round(x, 4) for x in b],
        "median_a": round(ma, 4),
        "median_b": round(mb, 4),
        "ratio": ratio and round(ratio, 3),
        "grade": grade,
    }


def rounds_for(measure: str, rounds: int) -> int:
    return min(rounds, BUILD_ROUNDS) if measure.startswith("build/") else rounds


def bisected(bench: Bench, frm: str, to: str, measure: str, rounds: int) -> dict:
    """The landing on `frm..to` (next's first parents) that made `measure` slower. A short range is measured
    landing by landing, every point in each round; a long one is halved toward the larger step."""
    landings = git(bench.repo, "rev-list", "--first-parent", "--reverse", f"{frm}..{to}").split()
    if not landings:
        raise SystemExit(f"{frm[:7]}..{to[:7]} holds no landing")
    points = [git(bench.repo, "rev-parse", frm), *landings]
    if len(landings) <= PER_LANDING:
        samples = bench.interleaved(points, measure, rounds_for(measure, rounds))
        medians = [statistics.median(s) for s in samples]
        steps = [
            {
                "landing": points[i],
                "subject": git(bench.repo, "log", "-1", "--format=%s", points[i]),
                "before": round(medians[i - 1], 4),
                "after": round(medians[i], 4),
                "change": round(medians[i] - medians[i - 1], 4),
            }
            for i in range(1, len(points))
        ]
        total = medians[-1] - medians[0]
        worst = max(steps, key=lambda s: s["change"])
        one = total > 0 and worst["change"] >= 0.5 * total
        return {
            "measure": measure,
            "from": points[0],
            "to": points[-1],
            "unit": unit_of(measure),
            "total": round(total, 4),
            "landing": worst["landing"] if one else None,
            "steps": steps,
        }
    # each round measures both ends and the middle, and goes on in the half that grew the most: it ends at
    # the landing with the largest step, not at the first one that grew
    lo, hi = 0, len(points) - 1
    while hi - lo > 1:
        mid = (lo + hi) // 2
        low, middle, high = (
            statistics.median(s) for s in bench.interleaved([points[lo], points[mid], points[hi]], measure, rounds_for(measure, rounds))
        )
        lo, hi = (lo, mid) if middle - low >= high - middle else (mid, hi)
    return {
        "measure": measure,
        "from": points[0],
        "to": points[-1],
        "unit": unit_of(measure),
        "landing": points[hi],
        "subject": git(bench.repo, "log", "-1", "--format=%s", points[hi]),
        "halved": True,
    }


def bench_of(args: argparse.Namespace, scratch: Path) -> Bench:
    return Bench(args.repo.resolve(strict=True), scratch, args.build, subjects(args.subject or []), args.libraries)


def command_reproduce(args: argparse.Namespace) -> None:
    given = json.loads(args.suspects.read_text()) if args.suspects else {}
    a, b = args.a or given.get("a"), args.b or given.get("b")
    measures = args.measure or given.get("measures", [])
    if not (a and b and measures):
        raise SystemExit("reproduce: give --a, --b and the measures (--measure or --suspects)")
    repo = args.repo.resolve(strict=True)
    a, b = git(repo, "rev-parse", a), git(repo, "rev-parse", b)
    result: dict = {"a": a, "b": b, "measures": {}}
    with tempfile.TemporaryDirectory(prefix="atlas-perf-ab-") as scratch:
        bench = bench_of(args, Path(scratch))
        try:
            for measure in measures:
                sa, sb = bench.interleaved([a, b], measure, rounds_for(measure, args.rounds))
                result["measures"][measure] = compared(measure, sa, sb)
                got = result["measures"][measure]
                print(f"{measure}: {amount(got['median_a'], got['unit'])} → {amount(got['median_b'], got['unit'])}, {got['grade']}")
            large = [m for m, g in result["measures"].items() if g["grade"] == "large"]
            if large and args.bisect_large:
                worst = max(large, key=lambda m: result["measures"][m]["ratio"])
                result["bisect"] = bisected(bench, a, b, worst, args.rounds)
        finally:
            bench.close()
    args.out.write_text(json.dumps(result, indent=1))


def command_bisect(args: argparse.Namespace) -> None:
    with tempfile.TemporaryDirectory(prefix="atlas-perf-bisect-") as scratch:
        bench = bench_of(args, Path(scratch))
        try:
            result = bisected(bench, getattr(args, "from"), args.to, args.measure, args.rounds)
        finally:
            bench.close()
    args.out.write_text(json.dumps(result, indent=1))
    print(bisect_text(result, None))


# concluding


def link(commit: str) -> str:
    server, repository = os.environ.get("GITHUB_SERVER_URL"), os.environ.get("GITHUB_REPOSITORY")
    return f"[`{commit[:7]}`]({server}/{repository}/commit/{commit})" if server and repository else f"`{commit[:7]}`"


def span(a: str, b: str) -> str:
    server, repository = os.environ.get("GITHUB_SERVER_URL"), os.environ.get("GITHUB_REPOSITORY")
    return f"[`{a[:7]}...{b[:7]}`]({server}/{repository}/compare/{a}...{b})" if server and repository else f"`{a[:7]}...{b[:7]}`"


def ab_table(measures: dict[str, dict]) -> list[str]:
    rows = ["| measure | A | B | B / A | |", "| --- | ---: | ---: | ---: | --- |"]
    for m, g in measures.items():
        word = {"large": "**reproduced, large**", "small": "reproduced", "noise": "not reproduced"}[g["grade"]]
        ratio = f"×{g['ratio']:.2f}" if g["ratio"] else "–"
        rows.append(f"| `{m}` | {amount(g['median_a'], g['unit'])} | {amount(g['median_b'], g['unit'])} | {ratio} | {word} |")
    return rows


def bisect_text(b: dict, url: str | None) -> str:
    lines = [f"Bisected `{b['measure']}` on {span(b['from'], b['to'])}:", ""]
    if b.get("halved"):
        lines.append(f"the landing that made it slower: {link(b['landing'])} {b['subject']}")
    else:
        if b["landing"]:
            step = next(s for s in b["steps"] if s["landing"] == b["landing"])
            lines.append(
                f"one landing made it slower: {link(b['landing'])} {step['subject']} ({amount(step['before'], b['unit'])} → {amount(step['after'], b['unit'])})"
            )
        else:
            lines.append(f"no single landing did: it grew by {amount(b['total'], b['unit'])} in steps, each landing's share below")
        lines += ["", "| landing | before | after | change |", "| --- | ---: | ---: | ---: |"]
        lines += [
            f"| {link(s['landing'])} {s['subject']} | {amount(s['before'], b['unit'])} | {amount(s['after'], b['unit'])} | {s['change']:+.2f} {b['unit']} |"
            for s in b["steps"]
        ]
    if url:
        lines += ["", f"The run: {url}"]
    return "\n".join(lines)


def command_conclude(args: argparse.Namespace) -> None:
    result = json.loads(args.reproduced.read_text())
    a, b, measures = result["a"], result["b"], result["measures"]
    reproduced = [m for m, g in measures.items() if g["grade"] != "noise"]
    large = [m for m, g in measures.items() if g["grade"] == "large"]
    if args.standing:
        title = args.release or a[:7]
        lines = [
            f"Standing of `next` at {link(b)} against {title} ({link(a)}), measured interleaved on one runner:",
            "",
            *ab_table(measures),
            "",
        ]
        if not reproduced:
            lines.append("Nothing is slower than in the release.")
        args.standing.write_text("\n".join(lines) + "\n")
        output(standing="true")
        return
    runs = history(args.data)
    if not runs or runs[-1]["commit"] != b:
        raise SystemExit(f"the history's last run is not {b[:7]}: conclude after record")
    runs[-1]["reproduced"] = sorted({*runs[-1].get("reproduced", []), *reproduced})
    runs[-1]["large"] = sorted({*runs[-1].get("large", []), *large})
    keep(args.data, runs)
    (args.data / "README.md").write_text(readme(runs, judged(runs[-1], runs[:-1]), args.data))
    lines = [f"#### Measured again: {link(a)} (A) against {link(b)} (B), interleaved on this runner", "", *ab_table(measures), ""]
    if not reproduced:
        lines.append("Not reproduced: the suspect run was the runner, not the code.")
    if args.summary:
        with open(args.summary, "a") as file:
            file.write("\n".join(lines) + "\n")
    else:
        print("\n".join(lines))
    if reproduced and args.alert:
        word = "much slower" if large else "slower"
        text = [
            f"The nightly measure of `next` found {len(reproduced)} measure(s) {word}, and reproduced it on one runner: {span(a, b)}.",
            "",
            *ab_table({m: measures[m] for m in reproduced}),
            "",
        ]
        if "bisect" in result:
            text += [bisect_text(result["bisect"], None), ""]
        elif reproduced:
            text += [
                f"To find the landing: run the workflow `perf` by hand with `mode: bisect`, `from: {a[:7]}`, `to: {b[:7]}` and the measure.",
                "",
            ]
        if args.url:
            text.append(f"The run: {args.url}")
        if args.data_url:
            text.append(f"History and charts: {args.data_url}")
        text += ["", "A slower measure is a question: about the change, the runner or the measure (`docs/reference/performance.md`)."]
        args.alert.write_text("\n".join(text) + "\n")
    output(alert="true" if reproduced else "false", urgent="true" if large else "false")


# the release's notes

RELEASE_BARS = [("read/atlas/cold/seconds", "Atlas reading itself, cold"), ("read/atlas/warm/seconds", "Atlas reading itself, warm")]


def night_of(runs: list[dict], repo: Path, ref: str) -> dict | None:
    """The latest night that measured `ref` or a commit before it."""
    for r in reversed(runs):
        if subprocess.run(["git", "-C", str(repo), "merge-base", "--is-ancestor", r["commit"], ref], capture_output=True).returncode == 0:
            return r
    return None


def command_release_bars(args: argparse.Namespace) -> None:
    runs = history(args.data)
    repo = args.repo.resolve(strict=True)
    nights = {ref: night_of(runs, repo, ref) for ref in (args.before, args.after)}
    if missing := [ref for ref, n in nights.items() if not n]:
        raise SystemExit(f"no night measured {', '.join(missing)} or a commit before it")
    bars = []
    for measure, label in RELEASE_BARS:
        for ref in (args.before, args.after):
            got = nights[ref]["metrics"].get(measure)
            if got:
                bars.append(f"{label}, {ref.removeprefix('v')} = {got['value']:.1f} s")
    nb, na = nights[args.before], nights[args.after]
    print(
        f"Bars: {'; '.join(bars)} | The nightly measure of `next` on the night before each release (`{nb['commit'][:7]}`, `{na['commit'][:7]}`), one reading a night (`docs/reference/performance.md`)."
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command_name", required=True)

    p = commands.add_parser("time", help="time a command once")
    p.add_argument("name")
    p.add_argument("--out", type=Path, required=True)
    p.set_defaults(func=command_time)

    p = commands.add_parser("read", help="time atlas-code view, cold and warm, on subjects")
    p.add_argument("--atlas", type=Path, required=True)
    p.add_argument("--subject", action="append", required=True, help="NAME=PATH")
    p.add_argument("--rounds", type=int, default=1)
    p.add_argument("--libraries", type=int, default=1)
    p.add_argument("--out", type=Path, required=True)
    p.set_defaults(func=command_read)

    p = commands.add_parser("tests", help="time each test binary")
    p.add_argument("--workspace", type=Path, default=Path("."))
    p.add_argument("--out", type=Path, required=True)
    p.set_defaults(func=command_tests)

    p = commands.add_parser("record", help="hold a run to the history, add it, draw the charts")
    p.add_argument("--data", type=Path, required=True)
    p.add_argument("--run", type=Path, required=True)
    p.add_argument("--commit", required=True)
    p.add_argument("--url")
    p.add_argument("--data-url")
    p.add_argument("--summary", type=Path)
    p.add_argument("--suspects", type=Path, help="where to write the suspect measures and the commit to measure them against")
    p.set_defaults(func=command_record)

    build = "cargo build --release --locked -p {package}"
    for name, func, text in [
        ("reproduce", command_reproduce, "measure two commits interleaved on this machine"),
        ("bisect", command_bisect, "find the landing that made a measure slower"),
    ]:
        p = commands.add_parser(name, help=text)
        p.add_argument("--repo", type=Path, default=Path("."))
        p.add_argument("--subject", action="append", help="NAME=PATH: relative, in each commit's tree; absolute, one for both")
        p.add_argument("--rounds", type=int, default=ROUNDS)
        p.add_argument("--libraries", type=int, default=1)
        p.add_argument(
            "--build",
            default=build,
            help="how a commit is built, with {tree}, {target} and {package}; its binary is then in {target}/release",
        )
        p.add_argument("--out", type=Path, required=True)
        p.set_defaults(func=func)
        if name == "reproduce":
            p.add_argument("--a")
            p.add_argument("--b")
            p.add_argument("--measure", action="append")
            p.add_argument("--suspects", type=Path)
            p.add_argument("--bisect-large", action="store_true", help="bisect the largest reproduced measure on the spot")
        else:
            p.add_argument("--from", required=True)
            p.add_argument("--to", required=True)
            p.add_argument("--measure", required=True)

    p = commands.add_parser("conclude", help="grade a reproduction: the history, the summary, the issue's text")
    p.add_argument("--data", type=Path)
    p.add_argument("--reproduced", type=Path, required=True)
    p.add_argument("--url")
    p.add_argument("--data-url")
    p.add_argument("--summary", type=Path)
    p.add_argument("--alert", type=Path)
    p.add_argument("--standing", type=Path, help="write the standing of next against a release instead")
    p.add_argument("--release", help="the release's name in the standing")
    p.set_defaults(func=command_conclude)

    p = commands.add_parser("release-bars", help="the Bars: line of a release's notes, from the series")
    p.add_argument("--data", type=Path, required=True)
    p.add_argument("--repo", type=Path, default=Path("."))
    p.add_argument("--before", required=True)
    p.add_argument("--after", required=True)
    p.set_defaults(func=command_release_bars)

    # what follows `--` is the command `time` runs, or the arguments `tests` gives cargo
    argv = sys.argv[1:]
    cut = argv.index("--") if "--" in argv else len(argv)
    args = parser.parse_args(argv[:cut])
    args.command = args.cargo = argv[cut + 1 :]
    if getattr(args, "rounds", 1) < 1:
        parser.error("--rounds must be positive")
    if args.command_name == "conclude" and not args.standing and not args.data:
        parser.error("conclude: --data, or --standing")
    args.func(args)


if __name__ == "__main__":
    main()
