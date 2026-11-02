# /// script
# requires-python = ">=3.11"
# ///
"""How fast Atlas builds, reads and tests, kept as a series per measure, and a signal when one gets slower.

    uv run tools/perf.py corpus (fetch | paths) --into DIR --for (perf | laws)
    uv run tools/perf.py time NAME --out RUN -- COMMAND...
    uv run tools/perf.py read --atlas ATLAS [--subject NAME=PATH ...] [--programs DIR] [--rounds N] [--timings FILE] --out RUN
    uv run tools/perf.py tests --workspace DIR --out RUN [-- CARGO_TEST_ARGS...]
    uv run tools/perf.py record --data DIR --run RUN --commit SHA [--url URL] [--summary FILE] [--suspects FILE]
    uv run tools/perf.py reproduce --repo DIR --a SHA --b SHA (--suspects FILE | --measure M ... | --table) [--subject NAME=PATH ...] [--programs DIR] [--timings FILE] --out FILE
    uv run tools/perf.py stages --timings FILE [--reproduced FILE]
    uv run tools/perf.py conclude --data DIR --reproduced FILE [--summary FILE] [--alert FILE] [--standing FILE]
    uv run tools/perf.py bisect --repo DIR --from SHA --to SHA --measure M [--subject NAME=PATH ...] --out FILE
    uv run tools/perf.py release-bars [--data DIR] [--reproduced FILE] --repo DIR --before TAG --after REF [--version V] [--out FILE]

`corpus` reads the corpus, `tools/corpus.txt`: the programs Atlas is measured on (`perf`) and its laws are sampled on
(`laws`), each at a fixed revision. `fetch` checks those of one reader out in DIR, one folder per name, a private one
with the secrets CORPUS_PRIVATE and CORPUS_TOKEN; `paths` names the folders. `--programs DIR` gives `read`,
`reproduce` and `bisect` the programs `perf` reads as subjects. Nothing printed or written of a private program
names its code: a reading that fails says so without Atlas's words, and its stages name only its libraries.

`time`, `read` and `tests` measure and add their measures to the run file RUN (JSON):

- `time` runs a command once: its wall time and the peak resident memory of its largest process.
- `read` draws each subject with `atlas-code view --timings`, cold (an empty cache and no index), warm (the
  same again) and unasked (again, with a file no reader asks about added), and serves it with `atlas-code serve`
  from a cold start, once a night (`--rounds` more for a median): the wall time, peak memory and the phases Atlas
  reports (read, check, flow, page), with how many bodies the reading linked, the program's and the libraries';
  of serve, the time to its first drawing, to its complete one, and what it spends drawing again after the same
  file. Each subject gets its card, from the graph `atlas-code analyze` reads of it: languages, files, lines,
  modules, functions, nodes, edges and library bodies linked, with the revision read.
- `tests` builds the test binaries once (`cargo test --no-run`), then runs each on its own as `cargo test`
  would, and times it: libtest's own timings are not stable Rust.

`record` holds a run to the history in DIR (`history.jsonl`, one line per run with its commit), adds it,
and draws DIR/README.md with the subjects' cards and its charts; a program of the corpus keeps its revision
under its name. A measure is suspect when it lies past its baseline, the median
of its last runs (those on the same CPU where there are enough), by more than the larger of a share of
the baseline, three spreads of the window and a floor. A suspect measure says nothing yet: `--suspects`
names each with the last commit that measured it within its baseline, for `reproduce`.

`reproduce` builds two commits and measures them interleaved (A B A B ...) on this machine, so that what
the machine does to one it does to the other: a measure is reproduced when every round of B is slower
than every round of A by more than the floor. Its size grades it: small below LARGE times A, large from
there; a large one is bisected on the spot, so that it names its commit. `conclude` writes the grades
into the history's last run, the summary and the text of the tracking issue (`--alert`, only for what was
reproduced), or the standing of `next` against a release (`--standing`). `bisect` finds the landing in a
range that made a measure slower, or gives each landing's share where no single one did. `reproduce --table`
measures the release's table: of each program of the corpus TABLE, of Atlas OF_ATLAS, with the cards of B's
readings. `release-bars` writes a release's table and lines (`docs/releases/<version>/bench.json`, the `Bench:`
line of its notes): each measure of the release before beside this one, from the interleaved run of the two
where it measured it, else from each release's night in the series, and each measure on the night of every
release since the series began. Any failure to measure ends with an error.

`--timings FILE`, given to `read`, `reproduce` or `bisect`, keeps what Atlas reported of each reading it
measured (`atlas-code view --timings`: the phases, the bodies linked, and the reading's stages per part and per
crate or package), by subject and kind for `read`, by measure and commit for the others. It goes to FILE and
never into a run: the series holds what a reading took, the file where its time went. `stages` writes such a
file as tables, each value the median over the readings: the phases, the stages, and the crates and packages
by the time reading them took, with the stages of each; of A and B side by side, given the reproduction.
"""

from __future__ import annotations

import argparse
import base64
import contextlib
import datetime
import html
import json
import os
import platform
import re
import shutil
import socket
import statistics
import subprocess
import sys
import tempfile
import tomllib
import time
import urllib.parse
import urllib.request
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


# the corpus

CORPUS = Path(__file__).resolve().parent / "corpus.txt"
READERS = ("perf", "laws")


@dataclass(frozen=True)
class Program:
    """A program of the corpus: its name, its repository (a URL, or `private`), the commit it is read at, and
    what reads it (`perf`, `laws`)."""

    name: str
    repository: str
    revision: str
    readers: tuple[str, ...]

    @property
    def private(self) -> bool:
        return self.repository == "private"


def corpus(file: Path = CORPUS) -> list[Program]:
    """The programs of the corpus list `file`, each checked: a full commit, readers it knows, a private program
    read by `perf` alone, and a name for one program only."""
    programs: list[Program] = []
    for number, line in enumerate(file.read_text().splitlines(), 1):
        line = line.split(" #", 1)[0].strip()
        if not line or line.startswith("#"):
            continue
        where = f"{file.name}:{number}"
        name, repository, revision, *readers = line.split()
        if not re.fullmatch(r"[0-9a-f]{40}", revision):
            raise SystemExit(f"{where}: {name} is read at {revision}: write the full commit, so that its series measures one program")
        if not readers or set(readers) - set(READERS):
            raise SystemExit(f"{where}: {name} is read by {' '.join(readers) or 'nothing'}: write {' and/or '.join(READERS)}")
        if repository == "private" and "laws" in readers:
            raise SystemExit(f"{where}: {name} is private, and the laws' report names the sites it samples: laws never reads it")
        if repository != "private" and not repository.startswith("https://"):
            raise SystemExit(f"{where}: {name}'s repository is {repository}: write its https URL, or private")
        if any(p.name == name for p in programs):
            raise SystemExit(f"{where}: {name} is named twice")
        programs.append(Program(name, repository, revision, tuple(readers)))
    return programs


def private(name: str) -> bool:
    """Whether `name` is a private program of the corpus: nothing this script prints or writes shows its code."""
    return any(p.name == name and p.private for p in corpus())


def secret_urls(text: str) -> dict[str, str]:
    """The secret CORPUS_PRIVATE: a line `NAME URL` per private program."""
    out = {}
    for line in text.splitlines():
        if line.strip() and not line.lstrip().startswith("#"):
            name, _, url = line.strip().partition(" ")
            out[name] = url.strip()
    return out


def fetch(program: Program, into: Path, urls: dict[str, str], token: str | None) -> Path:
    """`program` checked out at its revision in `into/<name>`, with its crates fetched where it has a Cargo.lock
    (Atlas reads a Rust program's libraries where Cargo keeps them). A private program is fetched with `token`, sent
    as a header to its host alone; what fails is said without its repository or git's words."""
    tree = into / program.name
    url, env = program.repository, dict(os.environ, GIT_TERMINAL_PROMPT="0")
    if program.private:
        url = urls.get(program.name, "")
        if not url or not token:
            lacking = " and ".join(s for s, has in (("CORPUS_PRIVATE (its line)", url), ("CORPUS_TOKEN", token)) if not has)
            raise SystemExit(
                f"{program.name} is private: its repository is its line in the secret CORPUS_PRIVATE, fetched with the "
                f"read-only token CORPUS_TOKEN, and this run lacks {lacking}"
            )
        parts = urllib.parse.urlsplit(url)
        header = base64.b64encode(f"x-access-token:{token}".encode()).decode()
        env |= {"GIT_CONFIG_COUNT": "1", "GIT_CONFIG_KEY_0": f"http.{parts.scheme}://{parts.netloc}/.extraheader"}
        env["GIT_CONFIG_VALUE_0"] = f"AUTHORIZATION: basic {header}"
    steps = [
        ["git", "init", "-q", str(tree)],
        ["git", "-C", str(tree), "fetch", "-q", "--depth", "1", url, program.revision],
        ["git", "-C", str(tree), "checkout", "-q", "FETCH_HEAD"],
    ]
    for step in steps:
        done = subprocess.run(step, env=env, capture_output=True, text=True, check=False)
        if done.returncode:
            said = "" if program.private else f": {done.stderr.strip()}"
            raise SystemExit(f"{program.name} could not be fetched at {program.revision[:12]}{said}")
    if (tree / "Cargo.lock").exists():
        done = subprocess.run(["cargo", "fetch", "--locked"], cwd=tree, capture_output=True, text=True, check=False)
        if done.returncode:
            said = "" if program.private else f": {done.stderr.strip()[-2000:]}"
            raise SystemExit(f"the crates of {program.name} could not be fetched{said}")
    return tree


def command_corpus(args: argparse.Namespace) -> None:
    programs = [p for p in corpus(args.corpus) if args.reader in p.readers]
    into = args.into.resolve()
    if args.action == "paths":
        print(":".join(str(into / p.name) for p in programs))
        return
    into.mkdir(parents=True, exist_ok=True)
    urls = secret_urls(os.environ.get("CORPUS_PRIVATE", ""))
    for p in programs:
        fetch(p, into, urls, os.environ.get("CORPUS_TOKEN") or None)
        print(f"{p.name}: {p.revision[:12]}" + (", private" if p.private else ""))


def programs_in(folder: Path | None, file: Path = CORPUS) -> list[tuple[str, Path]]:
    """The subjects of the corpus that `perf` reads, as `corpus fetch` lays them out in `folder`."""
    return [(p.name, folder / p.name) for p in corpus(file) if "perf" in p.readers] if folder else []


# measuring


def measured(
    command: list[str], *, cwd: Path | None = None, env: dict[str, str] | None = None, log: Path | None = None, quiet: bool = False
) -> tuple[float, float]:
    """Run `command` to its end: its wall time in seconds and the peak resident memory of its largest
    process in MB. Its output goes to `log`, else to this script's; a command that fails raises, with the end
    of its log unless `quiet` (a private program's log names its code)."""
    with open(log, "w") if log else contextlib.nullcontext() as out:
        start = time.perf_counter()
        process = subprocess.Popen(command, cwd=cwd, env=env, stdout=out, stderr=subprocess.STDOUT if out else None)
        # wait4, not wait: the child's own resource use, its peak memory among it
        _, status, usage = os.wait4(process.pid, 0)
        elapsed = time.perf_counter() - start
    if code := os.waitstatus_to_exitcode(status):
        tail = log.read_text(errors="replace")[-4000:] if log and not quiet else ""
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


def add(run: Path, metrics: dict[str, dict], cards: dict[str, dict] | None = None) -> None:
    """Add `metrics`, and the cards of the subjects they read, to the run file `run`, made when missing."""
    data = json.loads(run.read_text()) if run.exists() else {"runner": runner(), "metrics": {}}
    data["metrics"].update(metrics)
    if cards:
        data.setdefault("cards", {}).update(cards)
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


VIEWS = ("cold", "warm", "unasked")  # the views of a round, each run right after the one before it
KINDS = (*VIEWS, "serve")  # serve starts cold on its own
UNASKED = Path("docs") / "x.svg"  # a file no reader asks about: a picture
POLL = 0.02  # how often serve is asked what it has drawn, in seconds
QUIET = 5.0  # how long serve may take to start drawing after a file is added: its watch looks every second
STARTED = 1800.0  # how long a cold start of serve may take before the measure gives up, in seconds


@contextlib.contextmanager
def unasked(subject: Path):
    """`subject` with a file no reader asks about (UNASKED) while the context lasts, and as it was after."""
    file = subject / UNASKED
    if file.exists():
        raise SystemExit(f"{file} is there already: the measure adds it, and would take it away")
    made = [d for d in reversed(file.parents) if not d.exists()]
    file.parent.mkdir(parents=True, exist_ok=True)
    file.write_text('<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>\n')
    try:
        yield file
    finally:
        file.unlink(missing_ok=True)
        for d in reversed(made):
            d.rmdir()


def view(
    atlas: Path, name: str, subject: Path, libraries: int, scratch: Path, env: dict[str, str], reports: list[dict] | None = None
) -> dict[str, float]:
    """One `atlas-code view` of `subject`: its seconds, peak memory, phases and bodies linked; what Atlas reported, its
    stages among it, goes whole to `reports` where one is given."""
    timings = scratch / "timings.json"
    timings.unlink(missing_ok=True)
    command = [str(atlas), "--libraries", str(libraries), "view", "--no-open"]
    command += ["--out", str(scratch / "page.html"), "--timings", str(timings), str(subject)]
    hidden = private(name)
    seconds, peak = measured(command, env=env, log=scratch / "log.txt", quiet=hidden)
    if not timings.exists():
        raise SystemExit(f"{atlas} wrote no --timings for {name}")
    reported = json.loads(timings.read_text())
    if hidden:
        reported["stages"] = unnamed(reported.get("stages", []))
    if reports is not None:
        reports.append(dict(reported))
    linked = reported.pop("linked", {})
    reported.pop("stages", None)
    return {
        "seconds": seconds,
        "peak_rss": peak,
        **{f"phase/{p}": t for p, t in reported.items()},
        **{f"linked/{side}": float(n) for side, n in linked.items()},
    }


def unnamed(stages: list[dict]) -> list[dict]:
    """A private program's stages as they may be shown: the libraries' crates and packages by name, its own parts and
    its own code by none."""
    return [{**s, "part": "", "of": s.get("of") if s.get("library") else None} for s in stages]


def served(atlas: Path, name: str, subject: Path, libraries: int, scratch: Path, env: dict[str, str]) -> dict[str, float]:
    """One `atlas-code serve --no-open` of `subject`, asked `/api/drawn` every POLL seconds: the seconds until it
    first names a drawing (`first/seconds`) and until it names one and draws no more, the complete one
    (`seconds`); then, a file no reader asks about added, the seconds it spends drawing again, from the first
    answer that says it draws to the first that says it is done (`redraw/seconds`, 0 when it starts no
    drawing within QUIET); and the peak memory of the whole session."""
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        port = s.getsockname()[1]
    command = [str(atlas), "--libraries", str(libraries), "serve", "--no-open", "--exit-on-stdin-close", "--port", str(port), str(subject)]
    log = scratch / "serve.txt"
    url = f"http://127.0.0.1:{port}/api/drawn"
    direct = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    ended: list = []  # the server's resource use, once it has ended

    def gone() -> bool:
        if not ended:
            pid, _, usage = os.wait4(process.pid, os.WNOHANG)
            if pid:
                ended.append(usage)
        return bool(ended)

    hidden = private(name)

    def said() -> dict | None:
        """What the server says it has drawn; None while it does not listen yet."""
        if gone():
            tail = "" if hidden else "\n" + log.read_text(errors="replace")[-4000:]
            raise SystemExit(f"atlas-code serve of {name} ended on its own{tail}")
        try:
            with direct.open(url, timeout=30) as answer:
                state = json.loads(answer.read())
        except (OSError, ValueError):
            return None
        if state.get("error"):
            raise SystemExit(f"atlas-code serve of {name} could not draw" + ("" if hidden else f": {state['error']}"))
        return state

    def stop() -> None:
        """Close the server's standard input, which ends it (`--exit-on-stdin-close`), and wait for it."""
        if process.stdin and not process.stdin.closed:
            process.stdin.close()
        until = time.perf_counter() + 10
        while not gone() and time.perf_counter() < until:
            time.sleep(POLL)
        if not ended:
            process.kill()
            ended.append(os.wait4(process.pid, 0)[2])

    with open(log, "w") as out:
        start = time.perf_counter()
        process = subprocess.Popen(command, env=env, stdin=subprocess.PIPE, stdout=out, stderr=subprocess.STDOUT)
        try:
            first = complete = None
            while complete is None:
                state, now = said(), time.perf_counter() - start
                if state and state.get("drawn"):
                    first = now if first is None else first
                    if not state.get("drawing"):
                        complete, drawn = now, state["drawn"]
                elif now > STARTED:
                    raise SystemExit(f"atlas-code serve of {name} drew nothing in {STARTED:.0f} s")
                time.sleep(POLL)
            # the server ends before the file is taken away again, which would be a change of its own
            with unasked(subject):
                added, began, redraw = time.perf_counter(), None, None
                while redraw is None:
                    state, now = said(), time.perf_counter()
                    if state is None:
                        raise SystemExit(f"atlas-code serve of {name} stopped answering")
                    if began is None and (state.get("drawing") or state.get("drawn") != drawn):
                        began = now
                    if began is not None and not state.get("drawing"):
                        redraw = max(now - began, POLL)
                    elif began is None and now - added > QUIET:
                        redraw = 0.0
                    time.sleep(POLL)
                stop()
        finally:
            stop()
    # Linux counts the peak in KiB, macOS in bytes
    peak = ended[0].ru_maxrss / (1024 if sys.platform.startswith("linux") else 1024 * 1024)
    return {"seconds": complete, "first/seconds": first, "redraw/seconds": redraw, "peak_rss": peak}


def read_round(
    atlas: Path,
    name: str,
    subject: Path,
    libraries: int,
    scratch: Path,
    cache: Path,
    kinds: tuple[str, ...] = KINDS,
    reports: dict[str, list[dict]] | None = None,
) -> dict[str, dict[str, float]]:
    """A round of readings of `subject`, per kind its measures: `atlas-code view` cold (an empty cache and no index),
    warm (the same again), unasked (again, with a file no reader asks about added), and `atlas-code serve` from a
    cold start. A view of `kinds` brings the views before it, which it stands on. What Atlas reported of each view
    goes to `reports`, by kind, where one is given."""
    index = worktree_top(subject) / ".atlas" / "index"
    env = {**os.environ, "XDG_CACHE_HOME": str(cache), "ATLAS_NO_UPDATE_CHECK": "1"}

    def fresh() -> None:
        shutil.rmtree(index, ignore_errors=True)
        shutil.rmtree(cache, ignore_errors=True)

    got: dict[str, dict[str, float]] = {}
    views = VIEWS[: max((VIEWS.index(k) + 1 for k in kinds if k in VIEWS), default=0)]
    if views:
        fresh()
    for kind in views:
        with unasked(subject) if kind == "unasked" else contextlib.nullcontext():
            got[kind] = view(atlas, name, subject, libraries, scratch, env, None if reports is None else reports.setdefault(kind, []))
        if kind == "cold" and not any(index.rglob("*")):
            raise SystemExit(f"the cold reading of {name} made no index at {index}: a warm one would measure nothing")
    if "serve" in kinds:
        fresh()
        got["serve"] = served(atlas, name, subject, libraries, scratch, env)
    return got


LANGUAGES = {"python": "Python", "rust": "Rust", "typescript": "TypeScript"}


def card_of(graph: dict, root: Path) -> dict:
    """A program's shape, from the graph Atlas read of it (`atlas-code analyze`): its languages, the files of its
    own modules and their lines that are not blank, its modules and functions (methods among them), and of the
    whole graph, the libraries' fiber in it, the nodes, the edges and the library bodies linked. Counts alone:
    the card of a private program names nothing of it."""
    program = [n for n in graph["nodes"] if not n.get("library")]
    files = sorted({n["file"] for n in program if n["kind"] == "module" and n.get("file")})
    languages = {p["language"] for p in graph.get("parts", []) if p.get("language")}
    if not languages and isinstance(graph.get("language"), str):
        languages = {graph["language"]}
    lines = 0
    for file in files:
        path = Path(file) if Path(file).is_absolute() else root / file
        with contextlib.suppress(OSError):
            lines += sum(1 for line in path.read_text(errors="replace").splitlines() if line.strip())
    return {
        "languages": sorted(languages),
        "files": len(files),
        "lines": lines,
        "modules": sum(1 for n in program if n["kind"] == "module"),
        "functions": sum(1 for n in program if n["kind"] in ("function", "method")),
        "nodes": len(graph["nodes"]),
        "edges": len(graph["edges"]),
        "library_bodies": graph.get("stats", {}).get("linked_library_bodies", 0),
    }


def card_text(card: dict) -> str:
    """A card as a line: `Rust · 123 files · 31,092 lines · …`."""
    languages = " + ".join(LANGUAGES.get(x, x) for x in card.get("languages", [])) or "no language"
    counts = [(card[k], one, more) for k, one, more in CARD_WORDS if card.get(k) is not None]
    return " · ".join([languages, *(f"{n:,} {one if n == 1 else more}" for n, one, more in counts)])


CARD_WORDS = (
    ("files", "file", "files"),
    ("lines", "line", "lines"),
    ("modules", "module", "modules"),
    ("functions", "function", "functions"),
    ("nodes", "node", "nodes"),
    ("edges", "edge", "edges"),
    ("library_bodies", "library body", "library bodies"),
)


def card(atlas: Path, name: str, subject: Path, libraries: int, scratch: Path, env: dict[str, str], revision: str | None) -> dict:
    """The card of `subject` as `atlas` reads it at `libraries`, with the revision it was read at and whether the
    corpus pins it there."""
    graph = scratch / "graph.json"
    command = [str(atlas), "--libraries", str(libraries), "analyze", "--out", str(graph), str(subject)]
    measured(command, env=env, log=scratch / "analyze.txt", quiet=private(name))
    got = card_of(json.loads(graph.read_text()), subject)
    graph.unlink()
    pinned = next((p.revision for p in corpus() if p.name == name), None)
    return {**got, "revision": pinned or revision, "pinned": bool(pinned)}


def revision_of(subject: Path) -> str | None:
    done = subprocess.run(["git", "-C", str(subject), "rev-parse", "HEAD"], capture_output=True, text=True, check=False)
    return done.stdout.strip() or None


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
    cards: dict[str, dict] = {}
    reports: dict[str, dict[str, list[dict]]] = {}
    given = subjects(args.subject or []) + programs_in(args.programs)
    if not given:
        raise SystemExit("read: give --subject NAME=PATH or --programs DIR")
    with tempfile.TemporaryDirectory(prefix="atlas-perf-") as scratch:
        scratch = Path(scratch)
        for name, path in given:
            subject = path.resolve(strict=True)
            samples: dict[str, dict[str, list[float]]] = {kind: {} for kind in KINDS}
            for round_ in range(args.rounds):
                got_round = read_round(atlas, name, subject, args.libraries, scratch, scratch / f"cache-{name}", KINDS, reports.setdefault(name, {}))
                for kind, got in got_round.items():
                    for measure, value in got.items():
                        samples[kind].setdefault(measure, []).append(value)
                    more = f" (first drawing {got['first/seconds']:.2f} s, redraw {got['redraw/seconds']:.2f} s)" if kind == "serve" else ""
                    print(f"{name} {kind} {round_ + 1}/{args.rounds}: {got['seconds']:.2f} s{more}, peak {got['peak_rss']:.0f} MB")
            for kind, got in samples.items():
                for measure, values in got.items():
                    readings[f"read/{name}/{kind}/{measure}"] = metric(values, unit_of(measure))
            env = {**os.environ, "XDG_CACHE_HOME": str(scratch / f"cache-{name}"), "ATLAS_NO_UPDATE_CHECK": "1"}
            cards[name] = card(atlas, name, subject, args.libraries, scratch, env, revision_of(subject))
            print(f"{name}: {card_text(cards[name])}")
    add(args.out, readings, cards)
    if args.timings:
        args.timings.write_text(json.dumps(reports, indent=1))


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
    if unit == "bodies":
        return f"{value:,.0f}"
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
        or (measure.startswith("read/") and measure.count("/") == 3 and measure.endswith(("/seconds", "/peak_rss")))
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
    cards = last.get("cards", {})
    if cards:
        lines += [
            "",
            "## What is read",
            "",
            "Each subject by its card, as the last run read it: the files of its own modules and their lines that are",
            "not blank, its modules and functions, and of the graph with the libraries' part in it, the nodes, the",
            "edges and the library bodies linked. The programs of `tools/corpus.txt` are read at a fixed revision;",
            "`atlas` is Atlas at the commit measured.",
            "",
            "| subject | revision | card |",
            "| --- | --- | --- |",
        ]
        for name, c in sorted(cards.items()):
            revision = f"`{str(c.get('revision'))[:7]}`" + ("" if c.get("pinned") else ", the commit measured")
            lines.append(f"| `{name}` | {revision} | {card_text(c)} |")
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


def one_revision(cards: dict[str, dict], past: list[dict]) -> None:
    """A name of the corpus is one revision: a series that went on under the same name at another commit would
    measure two programs as one."""
    for name, c in cards.items():
        if not c.get("pinned"):
            continue
        before = next((r["cards"][name] for r in reversed(past) if name in r.get("cards", {})), None)
        if before and before.get("revision") != c.get("revision"):
            raise SystemExit(
                f"{name} was read at {str(before.get('revision'))[:12]} and now at {str(c.get('revision'))[:12]}: "
                "a program read at another commit gets another name in tools/corpus.txt"
            )


def last_cards(runs: list[dict]) -> dict[str, dict]:
    """The latest card of each subject the runs read."""
    out: dict[str, dict] = {}
    for r in runs:
        out.update(r.get("cards", {}))
    return out


def command_record(args: argparse.Namespace) -> None:
    run = json.loads(args.run.read_text())
    if not run.get("metrics"):
        raise SystemExit(f"{args.run} holds no measure")
    args.data.mkdir(parents=True, exist_ok=True)
    past = history(args.data)
    one_revision(run.get("cards", {}), past)
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
        "cards": run.get("cards", {}),
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
        # what Atlas reported of each reading measured, by measure and commit
        self.reports: dict[str, dict[str, list[dict]]] = {}

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
                if kind not in KINDS:
                    raise SystemExit(f"no reading {kind} to measure {measure}")
                reports: dict[str, list[dict]] = {}
                got = read_round(side.atlas, name, self.subject(side, name), self.libraries, scratch, scratch / "cache", (kind,), reports)
                self.reports.setdefault(measure, {}).setdefault(side.commit, []).extend(reports.get(kind, []))
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
    given = subjects(args.subject or []) + programs_in(args.programs)
    return Bench(args.repo.resolve(strict=True), scratch, args.build, given, args.libraries)


# the release's table: per program of the corpus, these readings; of Atlas, its build and its tests
TABLE = (
    ("cold/seconds", "cold read"),
    ("warm/seconds", "warm read"),
    ("serve/seconds", "served, to the complete picture"),
    ("cold/peak_rss", "peak memory"),
)
OF_ATLAS = (("build/release/seconds", "release build, clean"), ("tests/total/seconds", "test suite"))


def table_measures(programs: list[str]) -> list[str]:
    """The measures of the release's table, of `programs`."""
    return [f"read/{p}/{m}" for p in programs for m, _ in TABLE] + [m for m, _ in OF_ATLAS]


def command_reproduce(args: argparse.Namespace) -> None:
    given = json.loads(args.suspects.read_text()) if args.suspects else {}
    a, b = args.a or given.get("a"), args.b or given.get("b")
    measures = list(args.measure or given.get("measures", []))
    if args.table:
        measures += [m for m in table_measures([name for name, _ in programs_in(args.programs)]) if m not in measures]
    if not (a and b and measures):
        raise SystemExit("reproduce: give --a, --b and the measures (--measure, --table or --suspects)")
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
            if args.table:
                result["cards"] = cards_of(bench, b, sorted({m.split("/")[1] for m in measures if m.startswith("read/")}))
        finally:
            bench.close()
    args.out.write_text(json.dumps(result, indent=1))
    if args.timings:
        args.timings.write_text(json.dumps(bench.reports, indent=1))


def cards_of(bench: Bench, commit: str, names: list[str]) -> dict[str, dict]:
    """The card of each subject `names` as `commit`'s build reads it."""
    side = bench.side(commit)
    out = {}
    for name in names:
        with tempfile.TemporaryDirectory(dir=bench.scratch) as scratch:
            scratch = Path(scratch)
            env = {**os.environ, "XDG_CACHE_HOME": str(scratch / "cache"), "ATLAS_NO_UPDATE_CHECK": "1"}
            subject = bench.subject(side, name)
            out[name] = card(side.atlas, name, subject, bench.libraries, scratch, env, revision_of(subject))
            print(f"{name}: {card_text(out[name])}")
    return out


def command_bisect(args: argparse.Namespace) -> None:
    with tempfile.TemporaryDirectory(prefix="atlas-perf-bisect-") as scratch:
        bench = bench_of(args, Path(scratch))
        try:
            result = bisected(bench, getattr(args, "from"), args.to, args.measure, args.rounds)
        finally:
            bench.close()
    args.out.write_text(json.dumps(result, indent=1))
    if args.timings:
        args.timings.write_text(json.dumps(bench.reports, indent=1))
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
        if g["grade"] == "noise" and max(g["b"]) < min(g["a"]):
            word = "faster in every round"
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


# where a reading's time went

WHOLE = "(the whole part)"  # a stage of a part's reading that no one crate or package's code is read in
SHOWN = 12  # crates and packages a table names; the rest are summed in one row


def where(reported: dict) -> dict[str, dict]:
    """One reading's seconds by phase, by stage, by crate or package, and by crate or package and stage, with the
    bodies it linked of each crate or package: what `view --timings` reported, summed over the parts."""
    out: dict[str, dict] = {"phase": {}, "stage": {}, "of": {}, "grid": {}, "bodies": {}}
    out["phase"] = {k: float(v) for k, v in reported.items() if isinstance(v, (int, float))}
    for s in reported.get("stages", []):
        of = f"{s['of']} (library)" if s.get("library") else s.get("of") or WHOLE
        for table, key in (("stage", s["stage"]), ("of", of), ("grid", (of, s["stage"]))):
            out[table][key] = out[table].get(key, 0.0) + s["seconds"]
        if s["stage"] == "body":
            out["bodies"][of] = out["bodies"].get(of, 0.0) + s["count"]
    return out


def medians(readings: list[dict]) -> dict[str, dict]:
    """`where` of each reading, each value the median over them; a value a reading lacks is 0 there."""
    each = [where(r) for r in readings]
    out: dict[str, dict] = {}
    for table in ("phase", "stage", "of", "grid", "bodies"):
        keys = {k for e in each for k in e[table]}
        out[table] = {k: statistics.median(e[table].get(k, 0.0) for e in each) for k in keys}
    return out


def stage_tables(sides: dict[str, list[dict]]) -> list[str]:
    """Markdown: where the readings of each side (a label and its readings) went, the sides side by side, and of
    each side that read crates or packages, each of them by stage."""
    got = {label: medians(readings) for label, readings in sides.items()}
    labels = list(got)
    two = len(labels) == 2
    # the crates and packages by the most any side took reading them, its bodies as that side counted them
    most = {k: max(got.values(), key=lambda g: g["of"].get(k, 0.0)) for g in got.values() for k in g["of"]}

    def header(*first: str) -> list[str]:
        columns = [*first, *labels, *(["B − A"] if two else [])]
        return ["| " + " | ".join(columns) + " |", "| --- | " + " | ".join("---:" for _ in columns[1:]) + " |"]

    def cells(table: str, key) -> str:
        values = [g[table].get(key, 0.0) for g in got.values()]
        return " | ".join([amount(v, "s") for v in values] + ([f"{values[1] - values[0]:+.2f} s"] if two else []))

    phases = [p for p in ("read", "check", "flow", "page") if any(p in g["phase"] for g in got.values())]
    lines = [*header("phase"), *(f"| {p} | {cells('phase', p)} |" for p in phases)]
    if not any(g["stage"] for g in got.values()):
        return [*lines, "", "These readings report no stages: the build is older than the stages of `view --timings`."]
    stages = sorted({k for g in got.values() for k in g["stage"]}, key=lambda k: -max(g["stage"].get(k, 0.0) for g in got.values()))
    lines += ["", *header("stage"), *(f"| {k} | {cells('stage', k)} |" for k in stages)]
    order = sorted(most, key=lambda k: -most[k]["of"][k])
    shown, rest = order[:SHOWN], order[SHOWN:]

    def bodies(keys: list[str]) -> str:
        counted = [most[k]["bodies"][k] for k in keys if k in most[k]["bodies"]]
        return f"{sum(counted):.0f}" if counted else "–"

    lines += ["", *header("crate or package", "bodies")]
    lines += [f"| {k} | {bodies([k])} | {cells('of', k)} |" for k in shown]
    if rest:
        summed = [sum(g["of"].get(k, 0.0) for k in rest) for g in got.values()]
        change = [f"{summed[1] - summed[0]:+.2f} s"] if two else []
        lines.append(f"| {len(rest)} more | {bodies(rest)} | " + " | ".join([amount(v, "s") for v in summed] + change) + " |")
    for label, g in got.items():
        columns = [k for k in stages if any(g["grid"].get((o, k), 0.0) for o in shown if o != WHOLE)]
        if not columns:
            continue
        lines += ["", f"{label}, each crate or package by stage, in seconds:", ""]
        lines += ["| crate or package | " + " | ".join(columns) + " |", "| --- | " + " | ".join("---:" for _ in columns) + " |"]
        lines += [f"| {k} | " + " | ".join(f"{g['grid'].get((k, c), 0.0):.2f}" for c in columns) + " |" for k in shown if k != WHOLE]
    return lines


def command_stages(args: argparse.Namespace) -> None:
    kept = json.loads(args.timings.read_text())
    result = json.loads(args.reproduced.read_text()) if args.reproduced else None
    lines: list[str] = []
    if result:
        lines += [f"Measured interleaved on one runner: A {link(result['a'])}, B {link(result['b'])}.", "", *ab_table(result["measures"]), ""]
    for key, by in kept.items():
        if result:
            sides = {f"A `{result['a'][:7]}`": by.get(result["a"], []), f"B `{result['b'][:7]}`": by.get(result["b"], [])}
        else:
            sides = {f"`{commit[:7]}`" if len(commit) == 40 else commit: readings for commit, readings in by.items()}
        sides = {label: readings for label, readings in sides.items() if readings}
        if not sides:
            continue
        rounds = sorted({len(r) for r in sides.values()})
        lines += [f"#### `{key}`: where the reading went, the median of {' or '.join(map(str, rounds))} readings", ""]
        lines += [*stage_tables(sides), ""]
    print("\n".join(lines))


# the release's notes


def is_ancestor(repo: Path, commit: str, ref: str) -> bool:
    return subprocess.run(["git", "-C", str(repo), "merge-base", "--is-ancestor", commit, ref], capture_output=True, check=False).returncode == 0


def night_of(runs: list[dict], repo: Path, ref: str, earlier: str | None = None) -> dict | None:
    """The night of a release: the latest that measured `ref` or a commit before it, and neither `earlier`, the
    release before, nor a commit before that. A release no night measured between the two has none."""
    for r in reversed(runs):
        if is_ancestor(repo, r["commit"], ref) and not (earlier and is_ancestor(repo, r["commit"], earlier)):
            return r
    return None


def version_of(ref: str) -> tuple[int, ...] | None:
    m = re.fullmatch(r"v?(\d+)\.(\d+)\.(\d+)", ref)
    return tuple(map(int, m.groups())) if m else None


def releases(repo: Path, after: str, version: str) -> list[tuple[str, str]]:
    """Every stable release before `version`, oldest first, then `version` at the ref `after`: (name, ref)."""
    stable = sorted((t for t in git(repo, "tag", "-l", "v*").split() if version_of(t)), key=version_of)
    return [(t[1:], t) for t in stable if version_of(t) < version_of(version)] + [(version, after)]


def cell(measure: str, label: str, result: dict | None, before: dict | None, after: dict | None) -> dict:
    """A measure of the table, the release before against this one: from the interleaved run where it measured
    it, both on one runner; else from each release's night."""
    out = {"measure": measure, "label": label, "unit": unit_of(measure), "before": None, "after": None, "source": None}
    if result and measure in result["measures"]:
        got = result["measures"][measure]
        return {**out, "before": got["median_a"], "after": got["median_b"], "source": "interleaved"}
    b, a = ((n or {}).get("metrics", {}).get(measure) for n in (before, after))
    if a or b:
        out.update(before=b and b["value"], after=a and a["value"], source="nights")
    return out


def bench_data(
    programs: list[Program], runs: list[dict], nights: dict[str, dict | None], before: str, after: str, result: dict | None
) -> dict:
    """The release's speed, as `tools/release_page.py` draws it: a row per program of the corpus that `perf` reads, named
    by its card (a private one by its card alone), each measure of TABLE the release before beside this one, a row of
    Atlas's build and tests (OF_ATLAS); and each of those measures on the night of every release since the series began.
    `nights` maps each release, oldest first, to its night."""
    cards = {**last_cards(runs), **(result or {}).get("cards", {})}
    rows = []
    for p in programs:
        c = cards.get(p.name)
        languages = " + ".join(LANGUAGES.get(x, x) for x in (c or {}).get("languages", []))
        rows.append(
            {
                "name": None if p.private else p.name,
                "title": None if not p.private else f"A private {languages or ''} program".replace("  ", " "),
                "short": f"{languages}, {c['lines']:,} lines" if p.private and c else "a private program" if p.private else p.name,
                "card": c and {k: v for k, v in c.items() if k != "revision" or not p.private},
                "said": card_text(c) if c else None,
                "cells": [cell(f"read/{p.name}/{m}", label, result, nights.get(before), nights.get(after)) for m, label in TABLE],
            }
        )
    whole = [cell(m, label, result, nights.get(before), nights.get(after)) for m, label in OF_ATLAS]
    rows.append({"name": None, "title": "Atlas, built and tested", "short": "Atlas", "card": None, "said": None, "cells": whole})
    series = []
    for row in rows:
        for c in row["cells"]:
            points = [
                [release, night["metrics"][c["measure"]]["value"], night["commit"][:7], night["runner"].get("cpu", "")]
                for release, night in nights.items()
                if night and c["measure"] in night["metrics"]
            ]
            if points:
                series.append({"row": rows.index(row), "label": c["label"], "unit": c["unit"], "points": points})
    # a private program is known by its card alone: not even its label in the corpus goes to the page's data
    for row, p in zip(rows, programs):
        if p.private:
            row["cells"] = [{k: v for k, v in c.items() if k != "measure"} for c in row["cells"]]
    # a program that joined the corpus after both releases were measured has no row yet
    measured = [i for i, row in enumerate(rows) if any(c["before"] is not None or c["after"] is not None for c in row["cells"])]
    series = [{**s, "row": measured.index(s["row"])} for s in series if s["row"] in measured]
    rows = [rows[i] for i in measured]
    return {"before": before, "after": after, "releases": [r for r, n in nights.items() if n], "rows": rows, "series": series}


def command_release_bars(args: argparse.Namespace) -> None:
    repo = args.repo.resolve(strict=True)
    version = args.version or args.after.removeprefix("v")
    before = args.before.removeprefix("v")
    result = json.loads(args.reproduced.read_text()) if args.reproduced else None
    runs = history(args.data) if args.data else []
    shown = releases(repo, args.after, version)
    if (before, args.before) not in shown:
        raise SystemExit(f"{args.before} is no stable release before {version}")
    nights: dict[str, dict | None] = {}
    for i, (name, ref) in enumerate(shown):
        nights[name] = night_of(runs, repo, ref, shown[i - 1][1] if i else None)
    data = bench_data([p for p in corpus() if "perf" in p.readers], runs, nights, before, version, result)
    out = args.out or repo / "docs" / "releases" / version / "bench.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(data, indent=1) + "\n")
    sources = {c["source"] for row in data["rows"] for c in row["cells"]}
    said = []
    if "interleaved" in sources:
        rounds = sorted({len(g["a"]) for g in result["measures"].values()})
        run = f" ([the run]({args.run}))" if args.run else ""
        said.append(
            f"Each pair: `{result['a'][:7]}` ({before}) and `{result['b'][:7]}` ({version}) built and measured interleaved on one runner, "
            f"the median of {' or '.join(map(str, rounds))} rounds each{run}"
        )
    if "nights" in sources:
        named = ", ".join(f"`{nights[r]['commit'][:7]}`" for r in (before, version) if nights.get(r))
        said.append(f"{'where that run lacks a measure' if said else 'Each pair'}: the nightly measure of `next` on each release's night ({named})")
    said.append("The lines: every release's night since the series began, each on its night's runner (`docs/reference/performance.md`).")
    print(f"Bench: releases/{version}/bench.json | " + "; ".join(said[:-1]) + (". " if said[:-1] else "") + said[-1])
    print(f"wrote {out}", file=sys.stderr)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command_name", required=True)

    p = commands.add_parser("time", help="time a command once")
    p.add_argument("name")
    p.add_argument("--out", type=Path, required=True)
    p.set_defaults(func=command_time)

    p = commands.add_parser("read", help="time atlas-code view (cold, warm, unasked) and serve's cold start on subjects")
    p.add_argument("--atlas", type=Path, required=True)
    p.add_argument("--subject", action="append", help="NAME=PATH")
    p.add_argument("--programs", type=Path, help="the corpus's programs perf reads, as `corpus fetch` laid them out here")
    p.add_argument("--rounds", type=int, default=1)
    p.add_argument("--libraries", type=int, default=1)
    p.add_argument("--timings", type=Path, help="keep what Atlas reported of each reading here, its stages among it")
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
        p.add_argument("--programs", type=Path, help="the corpus's programs perf reads, as `corpus fetch` laid them out here")
        p.add_argument("--rounds", type=int, default=ROUNDS)
        p.add_argument("--libraries", type=int, default=1)
        p.add_argument(
            "--build",
            default=build,
            help="how a commit is built, with {tree}, {target} and {package}; its binary is then in {target}/release",
        )
        p.add_argument("--timings", type=Path, help="keep what Atlas reported of each reading measured here, its stages among it")
        p.add_argument("--out", type=Path, required=True)
        p.set_defaults(func=func)
        if name == "reproduce":
            p.add_argument("--a")
            p.add_argument("--b")
            p.add_argument("--measure", action="append")
            p.add_argument("--suspects", type=Path)
            p.add_argument("--table", action="store_true", help="the measures of the release's table, with the cards of B's readings")
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

    p = commands.add_parser("stages", help="where the readings of a --timings file went: phases, stages, crates and packages")
    p.add_argument("--timings", type=Path, required=True)
    p.add_argument("--reproduced", type=Path, help="the reproduction the readings were measured in: A and B side by side")
    p.set_defaults(func=command_stages)

    p = commands.add_parser("release-bars", help="the release's table and lines (`Bench:`), from an interleaved run and the series")
    p.add_argument("--data", type=Path, help="the series: the night of each release")
    p.add_argument("--reproduced", type=Path, help="an interleaved run of the two releases (`reproduce --table`), before as A")
    p.add_argument("--run", help="the link of that run, for the caption")
    p.add_argument("--repo", type=Path, default=Path("."))
    p.add_argument("--before", required=True, help="the release before, its tag (v0.70.0)")
    p.add_argument("--after", required=True, help="this release: its tag, or the ref it is cut at")
    p.add_argument("--version", help="this release's version, where --after is no tag")
    p.add_argument("--out", type=Path, help="where to write the data; default docs/releases/<version>/bench.json")
    p.set_defaults(func=command_release_bars)

    p = commands.add_parser("corpus", help="fetch the corpus's programs, or name where they are")
    p.add_argument("action", choices=["fetch", "paths"])
    p.add_argument("--into", type=Path, required=True)
    p.add_argument("--for", dest="reader", choices=READERS, required=True)
    p.add_argument("--corpus", type=Path, default=CORPUS)
    p.set_defaults(func=command_corpus)

    # what follows `--` is the command `time` runs, or the arguments `tests` gives cargo
    argv = sys.argv[1:]
    cut = argv.index("--") if "--" in argv else len(argv)
    args = parser.parse_args(argv[:cut])
    args.command = args.cargo = argv[cut + 1 :]
    if getattr(args, "rounds", 1) < 1:
        parser.error("--rounds must be positive")
    if args.command_name == "conclude" and not args.standing and not args.data:
        parser.error("conclude: --data, or --standing")
    if args.command_name == "release-bars" and not (args.data or args.reproduced):
        parser.error("release-bars: --data, or --reproduced")
    args.func(args)


if __name__ == "__main__":
    main()
