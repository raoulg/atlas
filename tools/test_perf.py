#!/usr/bin/env python3
"""The laws of the benchmark's corpus, its cards and a release's table (`tools/perf.py`, `tools/release_page.py`):
the corpus pins each program at one commit and never lets a private one be sampled; a card counts the program's own
code, whatever the libraries add to the graph, and names nothing of it; nothing written of a private program names
its code; a release's pair is taken from one runner where there is one; a release's night lies after the release
before; and the page draws every number of the table, never a private program's name.

    python3 tools/test_perf.py
"""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import perf  # noqa: E402
import release_page  # noqa: E402

SHA = "0123456789abcdef0123456789abcdef01234567"


def corpus_of(text: str) -> list[perf.Program]:
    with tempfile.TemporaryDirectory() as d:
        file = Path(d) / "corpus.txt"
        file.write_text(text)
        return perf.corpus(file)


def graph(library: bool) -> dict:
    """A small graph of a two-module program, with or without a library's fiber."""
    nodes = [
        {"id": "app", "kind": "module", "file": "app/__init__.py"},
        {"id": "app.main", "kind": "module", "file": "app/main.py"},
        {"id": "app.main.run", "kind": "function", "file": "app/main.py"},
        {"id": "app.main.Job", "kind": "class", "file": "app/main.py"},
        {"id": "app.main.Job.go", "kind": "method", "file": "app/main.py"},
    ]
    edges = [{"from": "app.main.run", "to": "app.main.Job", "kind": "instantiates"}]
    stats = {"linked_library_bodies": 0}
    if library:
        nodes += [
            {"id": "lib", "kind": "module", "file": "/site/lib/__init__.py", "library": True},
            {"id": "lib.f", "kind": "function", "file": "/site/lib/__init__.py", "library": True},
        ]
        edges += [{"from": "app.main.run", "to": "lib.f", "kind": "calls"}]
        stats = {"linked_library_bodies": 1}
    return {"parts": [{"id": "app", "language": "python"}], "nodes": nodes, "edges": edges, "stats": stats}


class Corpus(unittest.TestCase):
    def test_the_corpus_holds_its_own_laws(self):
        """tools/corpus.txt as it is: every program at a full commit, a name per program, a private one never sampled."""
        programs = perf.corpus()
        self.assertTrue(any("perf" in p.readers for p in programs))
        self.assertTrue(any("release" in p.readers for p in programs))
        for p in programs:
            if p.private:
                self.assertNotIn("laws", p.readers)

    def test_every_lock_a_program_commits_is_installed_by_its_tool_where_it_lies(self):
        """Fetching a lock's dependencies is one construct: each committed lock file, in any language and at any depth,
        names the command of its tool, run in its own directory; an uncommitted file (a lock in `node_modules`) is none."""
        with tempfile.TemporaryDirectory() as d:
            tree = Path(d)
            for f in ("Cargo.lock", "uv.lock", "web/package-lock.json", "web/pnpm-lock.yaml", "README.md"):
                (tree / f).parent.mkdir(parents=True, exist_ok=True)
                (tree / f).write_text("")
            (tree / "node_modules").mkdir()
            (tree / "node_modules" / "package-lock.json").write_text("")
            subprocess.run(["git", "init", "-q", str(tree)], check=True)
            subprocess.run(["git", "-C", str(tree), "add", "Cargo.lock", "uv.lock", "web", "README.md"], check=True)
            got = [(w.relative_to(tree).as_posix(), c[0] if c[0] != "corepack" else c[1]) for w, c in perf.locks_in(tree)]
            self.assertEqual(got, [(".", "cargo"), (".", "uv"), ("web", "npm"), ("web", "pnpm")])

    def test_a_private_program_is_never_sampled(self):
        with self.assertRaisesRegex(SystemExit, "laws never reads it"):
            corpus_of(f"secret private {SHA} perf laws\n")

    def test_a_program_is_read_at_a_full_commit(self):
        with self.assertRaisesRegex(SystemExit, "full commit"):
            corpus_of("x https://example.org/x 0123abc perf\n")

    def test_a_name_is_one_program(self):
        with self.assertRaisesRegex(SystemExit, "named twice"):
            corpus_of(f"x https://example.org/x {SHA} perf\nx https://example.org/y {SHA} laws\n")

    def test_a_private_program_unfetched_says_nothing_of_its_repository(self):
        program = perf.Program("p", "private", SHA, ("perf",))
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaisesRegex(SystemExit, "CORPUS_TOKEN") as missing:
                perf.fetch(program, Path(d), {}, None)
            self.assertNotIn("client-repository", str(missing.exception))
            with self.assertRaises(SystemExit) as failed:
                perf.fetch(program, Path(d), {"p": "file:///nowhere/client-repository"}, "token")
            self.assertNotIn("client-repository", str(failed.exception))
            self.assertNotIn("token", str(failed.exception).replace("CORPUS_TOKEN", ""))


class Card(unittest.TestCase):
    def card(self, library: bool) -> dict:
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "app").mkdir()
            (root / "app" / "__init__.py").write_text("\n")
            (root / "app" / "main.py").write_text("def run():\n\n    return Job()\n")
            return perf.card_of(graph(library), root)

    def test_the_libraries_fiber_leaves_the_programs_card(self):
        """What the libraries add to the graph changes its nodes, edges and library bodies, never the program's own counts."""
        alone, read = self.card(False), self.card(True)
        own = ("languages", "files", "lines", "modules", "functions")
        self.assertEqual({k: alone[k] for k in own}, {k: read[k] for k in own})
        self.assertGreater(read["nodes"], alone["nodes"])
        self.assertGreater(read["edges"], alone["edges"])
        self.assertGreater(read["library_bodies"], alone["library_bodies"])

    def test_a_card_counts(self):
        """A card names nothing of a program: counts, and the languages Atlas reads."""
        card = self.card(True)
        self.assertEqual(card["files"], 2)
        self.assertEqual(card["lines"], 2)
        self.assertEqual(card["functions"], 2)
        self.assertTrue(set(card["languages"]) <= set(perf.LANGUAGES))
        self.assertTrue(all(isinstance(v, int) for k, v in card.items() if k != "languages"))

    def test_a_private_programs_stages_name_its_libraries_alone(self):
        stages = [
            {"part": "client-backend", "stage": "parse", "of": "client_pkg", "library": False, "seconds": 1.0, "count": 3},
            {"part": "client-backend", "stage": "parse", "of": "fastapi", "library": True, "seconds": 2.0, "count": 4},
        ]
        shown = json.dumps(perf.unnamed(stages))
        self.assertNotIn("client", shown)
        self.assertIn("fastapi", shown)


class Series(unittest.TestCase):
    def test_a_name_keeps_its_revision(self):
        past = [{"cards": {"x": {"revision": "a" * 40, "pinned": True}, "atlas": {"revision": "c" * 40, "pinned": False}}}]
        perf.one_revision({"atlas": {"revision": "d" * 40, "pinned": False}}, past)
        with self.assertRaisesRegex(SystemExit, "another name"):
            perf.one_revision({"x": {"revision": "b" * 40, "pinned": True}}, past)

    def test_a_pair_from_one_runner_goes_before_two_nights(self):
        measure = "read/x/cold/seconds"
        result = {"measures": {measure: {"median_a": 2.0, "median_b": 1.0, "a": [2.0], "b": [1.0]}}}
        night = {"metrics": {measure: {"value": 9.0}}}
        got = perf.cell(measure, "cold read", result, night, night)
        self.assertEqual((got["before"], got["after"], got["source"]), (2.0, 1.0, "interleaved"))
        got = perf.cell(measure, "cold read", None, night, None)
        self.assertEqual((got["before"], got["after"], got["source"]), (9.0, None, "nights"))

    def test_the_summary_is_scale_free_and_symmetric(self):
        """The release's one number: twice as fast and twice as slow cancel, and 0.04 → 0.05 s weighs what 40 → 50 s weighs."""

        def cells(*pairs: tuple[float, float]) -> list[dict]:
            return [{"before": b, "after": a} for b, a in pairs]

        self.assertAlmostEqual(perf.summary_of(cells((1.0, 2.0), (2.0, 1.0)))["ratio"], 1.0)
        self.assertAlmostEqual(perf.summary_of(cells((0.04, 0.05)))["ratio"], perf.summary_of(cells((40.0, 50.0)))["ratio"])
        self.assertAlmostEqual(perf.summary_of(cells((0.04, 0.05)))["ratio"], 1.25)

    def test_a_measure_that_falls_to_zero_stays_out_of_the_mean(self):
        """A redraw the release no longer starts has no finite log ratio: the mean is that of the other measures, and
        the zero is counted beside it."""
        with_zero = perf.summary_of([{"before": 1.0, "after": 2.0}, {"before": 3.0, "after": 0.0}])
        without = perf.summary_of([{"before": 1.0, "after": 2.0}])
        self.assertEqual((with_zero["ratio"], with_zero["measures"], with_zero["zero"]), (without["ratio"], 1, 1))

    def test_a_change_is_told_from_noise_either_way(self):
        """A change by the rule of a reproduction, faster as well as slower: every round past every round, by more than the floor."""
        self.assertTrue(perf.changed_between([22.5, 22.6, 22.4], [20.4, 20.3, 20.4], "s"))
        self.assertTrue(perf.changed_between([20.4, 20.3, 20.4], [22.5, 22.6, 22.4], "s"))
        self.assertFalse(perf.changed_between([22.5, 20.2, 22.4], [20.4, 20.3, 20.4], "s"))
        self.assertFalse(perf.changed_between([1.23, 1.22, 1.24], [0.74, 0.75, 0.76], "s"))

    def test_a_releases_night_lies_after_the_release_before(self):
        """A release no night measured since the release before has no night: it never borrows that release's."""
        with tempfile.TemporaryDirectory() as d:
            repo = Path(d)

            def git(*args: str) -> str:
                return subprocess.run(["git", "-C", d, *args], check=True, capture_output=True, text=True).stdout.strip()

            git("init", "-q")
            commits = []
            for n in range(4):
                git("-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", str(n))
                commits.append(git("rev-parse", "HEAD"))
            git("tag", "v0.1.0", commits[1])
            def run(c: str) -> dict:
                return {"commit": c, "metrics": {}, "runner": {}}

            self.assertEqual(perf.night_of([run(commits[0])], repo, "v0.1.0")["commit"], commits[0])
            self.assertIsNone(perf.night_of([run(commits[0])], repo, commits[3], "v0.1.0"))
            got = perf.night_of([run(commits[0]), run(commits[2])], repo, commits[3], "v0.1.0")
            self.assertEqual(got["commit"], commits[2])


class Page(unittest.TestCase):
    def test_bars_that_compare_versions_are_drawn_as_pairs(self):
        """Labels `measure, version` or `measure before/after` form a group per measure with a side each; a list of
        parts that names no side stays a bar per label."""
        page = release_page.bars("Cold, 0.70.0 = 20 s; Cold, 0.71.0 = 18 s; Warm, 0.70.0 = 1.2 s; Warm, 0.71.0 = 0.8 s | c")
        self.assertEqual(page.count('class="gbar"'), 2)
        self.assertIn("b-legend", page)
        self.assertEqual(page.count('class="v s1'), 2)
        parts = release_page.bars("Linking = 22 s; Flow = 2.7 s; Everything else = 0.1 s")
        self.assertNotIn("gbar", parts)
        self.assertEqual(parts.count('class="bar"'), 3)


    def test_the_page_draws_every_number_and_no_private_name(self):
        programs = [perf.Program("public-x", "https://example.org/x", SHA, ("perf",)), perf.Program("hidden-y", "private", SHA, ("perf",))]
        card = {"languages": ["python", "typescript"], "files": 3, "lines": 61000, "modules": 3, "functions": 9}
        card |= {"nodes": 20, "edges": 30, "library_bodies": 5, "revision": SHA, "pinned": True}
        measures = {m: {"median_a": 3.0, "median_b": 2.0, "a": [3.0], "b": [2.0]} for m in perf.table_measures(["public-x", "hidden-y"])}
        result = {"a": "a" * 40, "b": "b" * 40, "measures": measures, "cards": {"public-x": card, "hidden-y": card}}
        night = {"commit": "c" * 40, "runner": {"cpu": "cpu"}, "metrics": {"read/public-x/cold/seconds": {"value": 4.0}}}
        data = perf.bench_data(programs, [], {"0.1.0": night, "0.2.0": night}, "0.1.0", "0.2.0", result)
        self.assertEqual(len(data["rows"]), len(programs))
        unmeasured = {"a": "a" * 40, "b": "b" * 40, "measures": {m: g for m, g in measures.items() if "hidden-y" not in m}}
        self.assertEqual(len(perf.bench_data(programs, [], {}, "0.1.0", "0.2.0", unmeasured)["rows"]), len(programs) - 1)
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / "bench.json").write_text(json.dumps(data))
            docs, release_page.DOCS = release_page.DOCS, Path(d)
            try:
                page = release_page.bench("bench.json | caption")
            finally:
                release_page.DOCS = docs
        self.assertNotIn("hidden-y", page)
        self.assertNotIn("hidden-y", json.dumps(data))
        self.assertIn("public-x", page)
        self.assertIn("61,000 lines", page)
        for row in data["rows"]:
            for c in row["cells"]:
                self.assertIn(release_page.measured(c["before"], c["unit"]), page)
                self.assertIn(release_page.measured(c["after"], c["unit"]), page)


if __name__ == "__main__":
    unittest.main()
