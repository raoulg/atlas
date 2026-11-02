# /// script
# requires-python = ">=3.11"
# dependencies = ["playwright"]
# ///
"""A small example program drawn by two builds of Atlas, for a release's notes: the picture of one
route by the release before and by this one, what only one of them draws outlined in its colour.

    uv run tools/release_example.py BEFORE AFTER EXAMPLE ROUTE OUTDIR [--open FACT,...] [--steps KEY,...] [--was ROUTE]

EXAMPLE is a program under the repository (`docs/releases/<version>/fixtures/<name>`, where Atlas reads no part of
its own), ROUTE the
route to draw, as the after build names it; `--was` names the route the before build starts the same program at,
where the change moves its start. It draws as `tools/release_report.py` draws a card of
what a user sees, and writes OUTDIR/before.png, OUTDIR/after.png and OUTDIR/example.json.
"""

import argparse
import json
import pathlib
import sys
import tempfile

from playwright.sync_api import sync_playwright

sys.dont_write_bytecode = True
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import release_report as rr  # noqa: E402


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    for name in ("before", "after", "example", "route", "outdir"):
        ap.add_argument(name)
    ap.add_argument("--open", default="", help="facts whose frames to open, as plans name them (`run/Expert()`)")
    ap.add_argument("--steps", default="", help="steps to open by their keys in Atlas's view format (`main/0`), in both builds")
    ap.add_argument("--was", default=None, help="the route the before build starts the program at, where it differs")
    a = ap.parse_args()
    builds = {"before": pathlib.Path(a.before).resolve(), "after": pathlib.Path(a.after).resolve()}
    out = pathlib.Path(a.outdir).resolve()
    out.mkdir(parents=True, exist_ok=True)
    facts = [{"fact": s} for s in a.open.split(",") if s]
    if a.steps:
        keys = [k for k in a.steps.split(",") if k]
        rr.opens_for = lambda tree, facts: (keys, [])
        facts = facts or [{"fact": keys[0]}]
    with tempfile.TemporaryDirectory() as tmp, sync_playwright() as p:
        br = rr.Browser(p, pathlib.Path(tmp))
        card = rr.user_card(br, builds, a.example, a.route, facts, out, "ex", a.was)
        br.browser.close()
    for side in ("before", "after"):
        made = out / f"ex-{side}.png"
        if made.is_file():
            made.rename(out / f"{side}.png")
    (out / "example.json").write_text(json.dumps(card, indent=1, ensure_ascii=False, default=list) + "\n")
    counts = card.get("difference", {}).get("counts", {})
    print(f"{a.example}: {card.get('says') or counts} {card.get('notes', '')} {card.get('failed', '')}")


if __name__ == "__main__":
    main()
