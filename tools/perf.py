# /// script
# requires-python = ">=3.11"
# ///
"""How fast Atlas builds, reads and tests, kept as a series per measure, and a signal when one gets slower.

    uv run tools/perf.py time NAME --out RUN -- COMMAND...
    uv run tools/perf.py read --atlas ATLAS --subject NAME=PATH ... [--rounds 3] --out RUN
    uv run tools/perf.py tests --workspace DIR --out RUN [-- CARGO_TEST_ARGS...]
    uv run tools/perf.py record --data DIR --run RUN --commit SHA [--url URL] [--summary FILE] [--alert FILE]

`time`, `read` and `tests` measure and add their measures to the run file RUN (JSON):

- `time` runs a command once: its wall time and the peak resident memory of its largest process.
- `read` draws each subject with `atlas view --timings`, cold (an empty cache and no index) and warm (the
  same again), `--rounds` times: the median wall time, peak memory and the phases Atlas reports (read,
  check, flow, page).
- `tests` builds the test binaries once (`cargo test --no-run`), then runs each on its own as `cargo test`
  would, and times it: libtest's own timings are not stable Rust.

`record` holds a run to the history in DIR (`history.jsonl`, one line per run with its commit), adds it,
and draws DIR/README.md with its charts. A measure is suspect when it lies past its baseline, the median
of its last runs (those on the same CPU where there are enough), by more than the larger of a share of
the baseline, three spreads of the window and a floor; it is slower when it was suspect on the run
before as well. A measure that is newly slower goes to `--alert`, the text of the tracking issue; the
verdict goes to `--summary`. Any failure to measure ends the script with an error.
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
import time
from dataclasses import dataclass
from pathlib import Path

WINDOW = 10  # runs in a baseline
LEAST = 3  # runs a baseline needs
SHARE = 0.20  # a measure is suspect past its baseline by more than this share of it...
SPREADS = 3.0  # ...and more than this many spreads (MAD scaled to a standard deviation) of the window...
FLOOR = {"s": 0.5, "MB": 50.0}  # ...and more than this
CHARTED = 90  # runs a chart shows


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


def command_read(args: argparse.Namespace) -> None:
    atlas = args.atlas.resolve(strict=True)
    readings: dict[str, dict] = {}
    with tempfile.TemporaryDirectory(prefix="atlas-perf-") as scratch:
        scratch = Path(scratch)
        for given in args.subject:
            name, _, path = given.partition("=")
            if not path:
                raise SystemExit(f"--subject {given}: write NAME=PATH")
            subject = Path(path).resolve(strict=True)
            index = worktree_top(subject) / ".atlas" / "index"
            samples: dict[str, dict[str, list[float]]] = {"cold": {}, "warm": {}}
            for round_ in range(args.rounds):
                cache = scratch / f"cache-{name}-{round_}"
                shutil.rmtree(index, ignore_errors=True)
                for kind in ("cold", "warm"):
                    timings = scratch / "timings.json"
                    timings.unlink(missing_ok=True)
                    env = {**os.environ, "XDG_CACHE_HOME": str(cache), "ATLAS_NO_UPDATE_CHECK": "1"}
                    command = [str(atlas), "--libraries", str(args.libraries), "view", "--no-open"]
                    command += ["--out", str(scratch / "page.html"), "--timings", str(timings), str(subject)]
                    seconds, peak = measured(command, env=env, log=scratch / "log.txt")
                    if not timings.exists():
                        raise SystemExit(f"{atlas} wrote no --timings for {name}")
                    if kind == "cold" and not any(index.rglob("*")):
                        raise SystemExit(f"the cold reading of {name} made no index at {index}: a warm one would measure nothing")
                    got = samples[kind]
                    got.setdefault("seconds", []).append(seconds)
                    got.setdefault("peak_rss", []).append(peak)
                    for phase, took in json.loads(timings.read_text()).items():
                        got.setdefault(f"phase/{phase}", []).append(took)
                    print(f"{name} {kind} {round_ + 1}/{args.rounds}: {seconds:.2f} s, peak {peak:.0f} MB")
            for kind, got in samples.items():
                for measure, values in got.items():
                    readings[f"read/{name}/{kind}/{measure}"] = metric(values, "MB" if measure == "peak_rss" else "s")
    add(args.out, readings)


def command_tests(args: argparse.Namespace) -> None:
    workspace = args.workspace.resolve(strict=True)
    build = subprocess.run(
        ["cargo", "test", "--locked", "--no-run", "--message-format=json", *args.cargo],
        cwd=workspace,
        stdout=subprocess.PIPE,
        text=True,
        check=False,
    )
    if build.returncode:
        raise SystemExit(f"cargo test --no-run failed with {build.returncode}")
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
    measures, total = {}, 0.0
    with tempfile.TemporaryDirectory(prefix="atlas-perf-tests-") as scratch:
        for name, executable, manifest_dir in sorted(binaries):
            # as `cargo test` runs it: in its package's directory, with the package's variables
            env = {**os.environ, "CARGO_MANIFEST_DIR": str(manifest_dir), "CARGO_PKG_NAME": name.split("/")[0]}
            seconds, _ = measured([str(executable)], cwd=manifest_dir, env=env, log=Path(scratch) / "log.txt")
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
    slower: bool = False
    newly: bool = False

    @property
    def change(self) -> float | None:
        return None if not self.baseline else self.value / self.baseline - 1


def history(data: Path) -> list[dict]:
    file = data / "history.jsonl"
    return [json.loads(line) for line in file.read_text().splitlines() if line.strip()] if file.exists() else []


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
            before = had[-1] if had else {}
            verdict.slower = verdict.suspect and measure in before.get("suspect", [])
            verdict.newly = verdict.slower and measure not in before.get("slower", [])
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
        mark = "slower" if v.slower else "suspect" if v.suspect else ""
        if v.baseline is None:
            mark = f"no baseline ({v.runs} of {LEAST} runs)"
        rows.append(
            f"| `{v.measure}` | {amount(v.value, v.unit)} | {amount(v.baseline, v.unit)} | {percent(v.change)} | {amount(v.threshold, v.unit)} | {mark} |"
        )
    return rows


def summary(run: dict, verdicts: list[Verdict], commit: str, url: str | None, data_url: str | None) -> str:
    slower = [v for v in verdicts if v.slower]
    suspect = [v for v in verdicts if v.suspect and not v.slower]
    judged_ = [v for v in verdicts if v.baseline is not None]
    lines = [f"### Performance of `{commit[:7]}`", ""]
    if slower:
        lines.append(f"**Slower**: {len(slower)} measure(s) lie past their baseline on this run and the run before.")
    if suspect:
        lines.append(f"**Suspect**: {len(suspect)} measure(s) lie past their baseline on this run; the next run says whether it holds.")
    if not slower and not suspect:
        lines.append(f"Nothing got slower: {len(judged_)} of {len(verdicts)} measures have a baseline, and each lies within it.")
    lines.append("")
    if slower or suspect:
        lines += table(slower + suspect) + [""]
    r = run["runner"]
    lines.append(f"Runner: {r['cpu']}, {r['cores']} cores, {r['os']}.")
    if data_url:
        lines.append(f"History and charts: {data_url}")
    lines += ["", "<details><summary>Every measure</summary>", "", *table(verdicts), "", "</details>", ""]
    return "\n".join(lines)


def alert(verdicts: list[Verdict], commit: str, past: list[dict], url: str | None, data_url: str | None) -> str:
    """The text of the tracking issue: what is newly slower, and between which commits it got so: the
    run before the first suspect one lay within the baseline, the first suspect one did not."""
    newly = [v for v in verdicts if v.newly]
    first = past[-1]["commit"]
    lines = [f"The nightly measure of `next` found {len(newly)} measure(s) slower on two runs in a row, `{first[:7]}` and `{commit[:7]}`."]
    if len(past) > 1:
        within = past[-2]["commit"]
        server, repository = os.environ.get("GITHUB_SERVER_URL"), os.environ.get("GITHUB_REPOSITORY")
        where = (
            f"[`{within[:7]}...{first[:7]}`]({server}/{repository}/compare/{within}...{first})"
            if server and repository
            else f"`{within[:7]}...{first[:7]}`"
        )
        lines.append(f"It came in {where}: the run of `{within[:7]}` lay within the baseline.")
    lines += [""] + table(newly) + [""]
    if url:
        lines.append(f"The run: {url}")
    if data_url:
        lines.append(f"History and charts: {data_url}")
    lines += ["", "A slower measure is a question: about the change, the runner or the measure (`docs/reference/performance.md`)."]
    return "\n".join(lines)


# drawing


def chart(points: list[tuple[str, float, bool]], baseline: float | None, unit: str) -> str:
    """A small line chart of a measure's runs, oldest left: a suspect run red, the baseline dashed."""
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
    for i, (commit, v, suspect) in enumerate(points):
        colour = "#d1495b" if suspect else "#1b8a9a"
        radius = 3 if suspect or i == len(points) - 1 else 1.5
        parts.append(
            f'<circle cx="{x(i):.1f}" cy="{y(v):.1f}" r="{radius}" fill="{colour}"><title>{commit[:7]}: {html.escape(amount(v, unit))}</title></circle>'
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
        "Written by `tools/perf.py record` from `history.jsonl`, one line per run; how it measures and judges is in",
        "`docs/reference/performance.md`. A red dot is a run past its baseline (dashed); a measure is slower when",
        "two runs in a row are.",
        "",
        f"Last run: {last['date']}, `{last['commit'][:7]}`, {r['cpu']}, {r['cores']} cores. Runs kept: {len(runs)}.",
    ]
    for title, vs in groups.items():
        lines += ["", f"## {title}", "", "| measure | last | baseline | change | runs |", "| --- | ---: | ---: | ---: | --- |"]
        for v in vs:
            cell = ""
            if charted(v.measure) or v.suspect:
                points = [
                    (r["commit"], r["metrics"][v.measure]["value"], v.measure in r.get("suspect", []))
                    for r in runs
                    if v.measure in r["metrics"]
                ]
                name = v.measure.replace("/", "-") + ".svg"
                (charts / name).write_text(chart(points[-CHARTED:], v.baseline, v.unit))
                cell = f"![{v.measure}](charts/{name})"
            mark = " **slower**" if v.slower else " suspect" if v.suspect else ""
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
    entry = {
        "commit": args.commit,
        "date": datetime.datetime.now(datetime.UTC).strftime("%Y-%m-%dT%H:%MZ"),
        "url": args.url,
        "runner": run["runner"],
        "metrics": run["metrics"],
        "suspect": [v.measure for v in verdicts if v.suspect],
        "slower": [v.measure for v in verdicts if v.slower],
    }
    with open(args.data / "history.jsonl", "a") as file:
        file.write(json.dumps(entry, sort_keys=True) + "\n")
    (args.data / "README.md").write_text(readme([*past, entry], verdicts, args.data))
    text = summary(run, verdicts, args.commit, args.url, args.data_url)
    if args.summary:
        with open(args.summary, "a") as file:
            file.write(text + "\n")
    else:
        print(text)
    newly = [v for v in verdicts if v.newly]
    if newly and args.alert:
        args.alert.write_text(alert(verdicts, args.commit, past, args.url, args.data_url))
    for v in verdicts:
        if v.slower:
            print(
                f"::warning title=slower::{v.measure}: {amount(v.value, v.unit)} against a baseline of {amount(v.baseline, v.unit)} ({percent(v.change)})"
            )
    if output := os.environ.get("GITHUB_OUTPUT"):
        with open(output, "a") as file:
            file.write(f"slower={'true' if newly else 'false'}\n")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command_name", required=True)

    p = commands.add_parser("time", help="time a command once")
    p.add_argument("name")
    p.add_argument("--out", type=Path, required=True)
    p.set_defaults(func=command_time)

    p = commands.add_parser("read", help="time atlas view, cold and warm, on subjects")
    p.add_argument("--atlas", type=Path, required=True)
    p.add_argument("--subject", action="append", required=True, help="NAME=PATH")
    p.add_argument("--rounds", type=int, default=3)
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
    p.add_argument("--alert", type=Path)
    p.set_defaults(func=command_record)

    # what follows `--` is the command `time` runs, or the arguments `tests` gives cargo
    argv = sys.argv[1:]
    cut = argv.index("--") if "--" in argv else len(argv)
    args = parser.parse_args(argv[:cut])
    args.command = args.cargo = argv[cut + 1 :]
    if getattr(args, "rounds", 1) < 1:
        parser.error("--rounds must be positive")
    args.func(args)


if __name__ == "__main__":
    main()
