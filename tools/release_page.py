# /// script
# requires-python = ">=3.11"
# ///
"""The release notes of Atlas as pages of the documentation site: one page per stable version, and a
page that lists them.

    uv run tools/release_page.py            # every docs/releases/<version>.md
    uv run tools/release_page.py 0.68.0     # one

A release's notes are `docs/releases/<version>.md`: a head (`version`, `date`, empty until the tag),
a paragraph that says what the release is, and its main points, each a `## ` heading with a few
paragraphs (code in fences), an `Issues:` line, and the point's pictures:

- `Atlas: #147, #175`: the issue's cards of `tools/release_report.py`, its program drawn by the release
  before and by this one (what only one of them draws outlined in its colour), and Atlas's own code
  where the change is. `--take <report dir> <version>` copies what the notes name of a report into
  `docs/releases/<version>/report/`.
- `Diagram: releases/<version>/x.svg | caption`: a drawing of the point's structure, inline, so that it
  takes the page's colours in the light and the dark, with what it shows beneath it.
- `Bars: label = 18.9 s; label = 0.14 s | caption`: a bar chart of measured numbers.
- `Pictures: changes/x.jpg`: screenshots, from the notes each issue leaves in `docs/changes/`.

A `Draw:` line says which picture is still to be made for a point; the page leaves it out and this
tool names it, so that the release is not tagged with a point that shows nothing. The page is
`docs/releases/<version>/index.html`, in the site's own style; `docs/releases/index.html` lists the
releases, newest first. Pages publishes `docs/` from `main`, so a page goes live with the promotion
that carries it, at `https://raoulg.github.io/atlas/releases/<version>/`.
"""

import html
import pathlib
import re
import sys

DOCS = pathlib.Path(__file__).resolve().parent.parent / "docs"
RELEASES = DOCS / "releases"
ISSUES = "https://github.com/raoulg/atlas/issues/"

STYLE = """
:root{--bg:#F4F1EC;--panel:#FFFFFF;--ink:#111111;--ink-2:#5A544D;--ink-3:#857D73;--line:#E2DCD3;--accent:#C43A14;
  --sans:"Outfit","Helvetica Neue",Helvetica,system-ui,sans-serif;--mono:"JetBrains Mono",ui-monospace,SFMono-Regular,Menlo,monospace}
@media(prefers-color-scheme:dark){:root{--bg:#141210;--panel:#1B1815;--ink:#F4F1EC;--ink-2:#B8B0A5;--ink-3:#8A8178;--line:#2B2621}}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--ink);font-family:var(--sans);font-size:17px;line-height:1.55}
a{color:inherit}
main{max-width:1040px;margin:0 auto;padding:48px 16px 96px}
header{display:flex;align-items:center;gap:16px;margin-bottom:48px}
header img{width:44px;height:44px}
header .name{font-size:22px;font-weight:600;letter-spacing:-.01em}
header nav{margin-left:auto;display:flex;gap:20px;font-family:var(--mono);font-size:13px}
header nav a{text-decoration:none;color:var(--ink-2)}
header nav a:hover{color:var(--ink)}
.eyebrow{font-family:var(--mono);font-size:12px;letter-spacing:.08em;text-transform:uppercase;color:var(--accent);margin:0}
h1{font-size:44px;font-weight:500;letter-spacing:-.02em;line-height:1.1;margin:10px 0 18px}
.lede{font-size:19px;color:var(--ink-2);max-width:66ch;margin:0 0 56px}
section{border-top:1px solid var(--line);padding:40px 0 8px}
section h2{font-size:28px;font-weight:500;letter-spacing:-.01em;margin:0 0 14px;line-height:1.2}
section p,section li{max-width:70ch;color:var(--ink-2)}
section p{margin:0 0 14px}
code{font-family:var(--mono);font-size:.86em;background:var(--panel);border:1px solid var(--line);padding:0 4px}
.issues{font-family:var(--mono);font-size:12.5px;color:var(--ink-3);margin-top:6px}
.issues a{color:var(--ink-3);text-decoration:none}
.issues a:hover{color:var(--accent)}
.pictures{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,420px),1fr));gap:16px;margin:22px 0 10px}
.pictures a{display:block;background:var(--panel);border:1px solid var(--line);padding:6px}
.pictures img{display:block;width:100%;height:auto}
.list a{display:flex;gap:18px;align-items:baseline;padding:14px 0;border-top:1px solid var(--line);text-decoration:none}
.list .v{font-family:var(--mono);font-size:15px;min-width:90px}
.list .t{color:var(--ink-2)}
footer{margin-top:64px;font-family:var(--mono);font-size:12px;color:var(--ink-3)}
:root{--add:#2E8B57;--del:#C0361C;--edit:#B26A00;--key:#1D5A73}
@media(prefers-color-scheme:dark){:root{--add:#6CC08C;--del:#F2643C;--edit:#E0A24A;--key:#86B9C9}}
.legend{display:flex;flex-wrap:wrap;gap:8px 18px;font-size:14px;color:var(--ink-2);margin:-30px 0 48px}
.legend span{display:inline-flex;align-items:center;gap:7px}
.swatch{width:18px;height:12px;border:3px solid;border-radius:2px}
.card{background:var(--panel);border:1px solid var(--line);padding:16px 18px;margin:20px 0}
.card h3{font-size:16px;font-weight:500;margin:0 0 12px}
.card h3 a{text-decoration:none;color:var(--ink-3);font-family:var(--mono);font-size:13px;margin-right:6px}
.pair{display:grid;gap:12px}
.pair figure{margin:0}
.pair .shot{overflow-x:auto;border:1px solid var(--line);background:var(--bg);padding:6px}
.pair .shot pre{border:0;background:none;margin:0;padding:4px;font-size:12px;max-width:none;max-height:340px}
@media(min-width:900px){.pair:has(pre){grid-template-columns:1fr 1fr}}
.pair img{display:block;max-width:100%;height:auto}
figcaption{font-size:13.5px;color:var(--ink-2);margin-top:6px}
.tag{font-family:var(--mono);font-size:11.5px;padding:1px 6px;border:1.5px solid;margin-right:6px}
.tag.before{color:var(--del)}.tag.after{color:var(--add)}
.where{font-family:var(--mono);font-size:12px;color:var(--ink-3);margin:0 0 8px}
details{margin-top:12px}
summary{cursor:pointer;font-size:13.5px;color:var(--ink-2)}
pre{font-family:var(--mono);font-size:13px;background:var(--panel);border:1px solid var(--line);padding:12px 14px;overflow:auto;max-width:70ch}
pre code{background:none;border:0;padding:0}
.hero{margin:6px 0 24px;border:1px solid var(--line);background:var(--panel);padding:8px}
.hero img{display:block;width:100%;height:auto}
.example{margin:18px 0 6px}
.example pre{margin:0 0 14px}
@media(min-width:900px){.pair{grid-template-columns:1fr 1fr;align-items:start}}
.pair.stack{grid-template-columns:1fr}
section ul{padding-left:20px;margin:0 0 16px}
section li{margin:4px 0}
.diagram{margin:22px 0;background:var(--panel);border:1px solid var(--line);padding:14px}
.diagram svg{display:block;width:100%;height:auto;max-height:340px}
.diagram figcaption{font-size:14px;color:var(--ink-3);margin-top:10px}
.d-frame{fill:none;stroke:var(--ink-3);stroke-width:1.5}
.d-box{fill:var(--bg);stroke:var(--line);stroke-width:1.5}
.d-key{stroke:var(--key);stroke-width:2}
.d-engine{fill:var(--panel);stroke:var(--ink-3)}
.d-ground{fill:var(--line);stroke:none}
.d-title{font:600 15px var(--sans);fill:var(--ink)}
.d-name{font:500 14px var(--mono);fill:var(--ink)}
.d-note{font:13px var(--sans);fill:var(--ink-2)}
.d-label{font:13px var(--sans);fill:var(--ink-2)}
.d-arrow{fill:none;stroke:var(--ink-3);stroke-width:1.6;marker-end:url(#d-head)}
.d-before{stroke:var(--del)}.d-after{stroke:var(--add)}
.d-law{font:28px var(--sans);fill:var(--key)}
.d-broken{fill:var(--del)}
.d-axis{stroke:var(--ink-3);stroke-width:1.4;fill:none}.d-grid{stroke:var(--line);stroke-dasharray:4 4}
.d-curve{fill:none;stroke:var(--key);stroke-width:3}.d-dot{fill:var(--key)}
.bars{margin:22px 0;background:var(--panel);border:1px solid var(--line);padding:16px 18px;max-width:760px}
.bar{display:grid;grid-template-columns:minmax(120px,30%) 1fr;gap:12px;align-items:center;margin:8px 0}
.bar .l{font-size:14px;color:var(--ink-2)}
.bar .v{height:22px;background:var(--key);min-width:2px;position:relative}
.bar .v span{position:absolute;left:calc(100% + 8px);top:1px;font-family:var(--mono);font-size:13px;color:var(--ink);white-space:nowrap}
.bars .cap{font-size:13.5px;color:var(--ink-2);margin-top:10px}
"""


def inline(text: str) -> str:
    """A line of the notes as HTML: `code`, **strong**, [links](url) and #issues; the rest escaped."""
    out, at = [], 0
    for m in re.finditer(r"`([^`]+)`|\*\*([^*]+)\*\*|\[([^\]]+)\]\(([^)]+)\)|(?<![\w&])#(\d+)\b", text):
        out.append(html.escape(text[at : m.start()]))
        code, strong, label, url, issue = m.groups()
        if code is not None:
            out.append(f"<code>{html.escape(code)}</code>")
        elif strong is not None:
            out.append(f"<strong>{html.escape(strong)}</strong>")
        elif label is not None:
            out.append(f'<a href="{html.escape(url)}">{html.escape(label)}</a>')
        else:
            out.append(f'<a href="{ISSUES}{issue}">#{issue}</a>')
        at = m.end()
    out.append(html.escape(text[at:]))
    return "".join(out)


def blocks(text: str) -> list[str]:
    """Paragraphs, lists and fenced code of `text`, as HTML."""
    out = []
    parts = re.split(r"(?ms)^```[^\n]*\n(.*?)^```\s*$", text.strip())
    for n, part in enumerate(parts):
        if n % 2:
            out.append(f"<pre><code>{html.escape(part.rstrip())}</code></pre>")
        else:
            out += prose(part)
    return out


def prose(text: str) -> list[str]:
    """Paragraphs and lists of `text`, as HTML."""
    out = []
    for chunk in re.split(r"\n\s*\n", text.strip()):
        lines = [line.strip() for line in chunk.splitlines() if line.strip()]
        if not lines:
            continue
        if all(line.startswith("- ") for line in lines):
            out.append("<ul>" + "".join(f"<li>{inline(line[2:])}</li>" for line in lines) + "</ul>")
        elif lines[0].startswith("- "):
            head = [line for line in lines if not line.startswith("- ")]
            out.append(f"<p>{inline(' '.join(head))}</p>" if head else "")
            out.append("<ul>" + "".join(f"<li>{inline(line[2:])}</li>" for line in lines if line.startswith("- ")) + "</ul>")
        else:
            out.append(f"<p>{inline(' '.join(lines))}</p>")
    return out


def read(path: pathlib.Path) -> dict:
    """A release's notes: its head, its opening paragraph and its points."""
    text = path.read_text()
    head, body = {}, text
    if text.startswith("---\n"):
        raw, body = text[4:].split("\n---\n", 1)
        head = dict(line.split(":", 1) for line in raw.splitlines() if ":" in line)
        head = {k.strip(): v.strip() for k, v in head.items()}
    parts = re.split(r"(?m)^## ", body)
    points = []
    for part in parts[1:]:
        title, rest = (part.split("\n", 1) + [""])[:2]
        fields = {}

        def field(m):
            fields[m.group(1).lower()] = " ".join(m.group(2).split())
            return ""

        keys = "Issues|Pictures|Draw|Atlas|Diagram|Bars|Hero|Example|Compare"
        rest = re.sub(rf"(?ms)^({keys}): (.*?)(?=\n\s*\n|\n(?:{keys}): |\Z)", field, rest)
        points.append({"title": re.sub(r"^\d+\.\s*", "", title.strip()), "text": rest, **fields})
    release = {"version": head.get("version", path.stem), "date": head.get("date", ""), "lede": parts[0].strip(), "points": points}
    report = RELEASES / release["version"] / "report" / "report.json"
    if report.is_file():
        import json

        release["builds"] = json.loads(report.read_text()).get("builds", {})
    release.setdefault("builds", {"before": f"atlas {head.get('before', 'before')}", "after": f"atlas {release['version']}"})
    return release


def page(release: dict) -> str:
    """The page of one release."""
    v = html.escape(release["version"])
    sections = []
    for point in release["points"]:
        body = blocks(point["text"])
        if point.get("hero"):
            body.insert(0, f'<figure class="hero"><img src="../../{html.escape(point["hero"])}" alt=""></figure>')
        if point.get("example"):
            body.append(example(release, point["example"]))
        if point.get("compare"):
            body.append(compare(point["compare"]))
        drawings, _, caption = point.get("diagram", "").partition("|")
        for diagram in [d.strip() for d in drawings.split(",") if d.strip()]:
            said = f"<figcaption>{inline(caption.strip())}</figcaption>" if caption.strip() else ""
            body.append(f'<figure class="diagram">{svg(DOCS / diagram)}{said}</figure>')
        if point.get("bars"):
            body.append(bars(point["bars"]))
        for n in re.findall(r"#(\d+)", point.get("atlas", "")):
            body.append(card(release, int(n)))
        pictures = [p.strip() for p in point.get("pictures", "").split(",") if p.strip()]
        if pictures:
            body.append(
                '<div class="pictures">'
                + "".join(f'<a href="../../{html.escape(p)}"><img src="../../{html.escape(p)}" alt="" loading="lazy"></a>' for p in pictures)
                + "</div>"
            )
        if point.get("issues"):
            body.append(f'<p class="issues">{inline(point["issues"])}</p>')
        sections.append(f'<section><h2>{inline(point["title"])}</h2>{"".join(body)}</section>')
    when = f" · {html.escape(release['date'])}" if release["date"] else ""
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Atlas {v}</title>
<meta name="description" content="What changed in Atlas {v}">
<link rel="icon" href="../../logo.svg">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link href="https://fonts.googleapis.com/css2?family=Outfit:wght@400;500;600&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
<style>{STYLE}</style>
</head>
<body>
<main>
<header><img src="../../logo.svg" alt=""><span class="name">Atlas</span>
<nav><a href="../">Releases</a><a href="../../">Atlas</a><a href="https://github.com/raoulg/atlas/releases/tag/v{v}">Download</a></nav></header>
<p class="eyebrow">Release {v}{when}</p>
<h1>What changed in Atlas {v}</h1>
<div class="lede">{"".join(blocks(release["lede"]))}</div>
<div class="legend"><span><i class="swatch" style="border-color:var(--del)"></i>only in the release before</span><span><i class="swatch" style="border-color:var(--add)"></i>only in {v}</span><span><i class="swatch" style="border-color:var(--edit)"></i>its text changed, its arrow not</span></div>
{"".join(sections)}
<footer>Made by tools/release_page.py from docs/releases/{v}.md and the notes in docs/changes.</footer>
</main>
</body>
</html>
"""


def example(release: dict, name: str) -> str:
    """A small program of `fixtures/<name>` (fixtures: Atlas reads no part there, so the program stays out
    of Atlas's own picture) and the picture of it by the release before and by this one
    (`tools/release_example.py` draws them into `img/<name>-before.png` and `-after.png`)."""
    folder = RELEASES / release["version"] / "fixtures" / name
    source = next((p for p in sorted(folder.rglob("*.py")) if "typings" not in p.parts), None)
    code = source.read_text().split("\n\n\ndef main(")[0].strip() if source else ""
    builds = {side: html.escape(v.split()[1]) if len(v.split()) > 1 else side for side, v in release.get("builds", {}).items()}
    labels = {"before": builds.get("before", "before"), "after": builds.get("after", release["version"])}
    figures = "".join(
        f'<figure><div class="shot"><img src="img/{html.escape(name)}-{side}.png" alt="" loading="lazy"></div>'
        f'<figcaption><span class="tag {side}">{labels[side]}</span></figcaption></figure>'
        for side in ("before", "after")
    )
    return f'<div class="example"><pre><code>{html.escape(code)}</code></pre><div class="pair">{figures}</div></div>'


def compare(spec: str) -> str:
    """`img = label; img = label`: pictures one above the other, each at the page's width with its label."""
    figures = []
    for item in [i for i in spec.split(";") if "=" in i]:
        path, label = (x.strip() for x in item.split("=", 1))
        figures.append(
            f'<figure><div class="shot"><img src="../../{html.escape(path)}" alt="" loading="lazy"></div>'
            f'<figcaption><span class="tag after">{inline(label)}</span></figcaption></figure>'
        )
    return f'<div class="pair stack">{"".join(figures)}</div>'


def svg(path: pathlib.Path) -> str:
    """A drawing of the notes, inline, with the arrowhead its arrows use."""
    head = ('<defs><marker id="d-head" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto">'
            '<path d="M0 0 L10 5 L0 10 z" style="fill:var(--ink-3)"/></marker></defs>')
    text = path.read_text()
    return re.sub(r"(<svg[^>]*>)", lambda m: m.group(1) + head, text, count=1)


def bars(spec: str) -> str:
    """`label = 18.9 s; label = 0.14 s | caption` as bars of one scale."""
    data, _, caption = spec.partition("|")
    rows = []
    for item in [i for i in data.split(";") if "=" in i]:
        label, value = (x.strip() for x in item.split("=", 1))
        number = float(re.match(r"[\d.]+", value).group(0))
        rows.append((label, value, number))
    top = max(n for _, _, n in rows) or 1
    out = "".join(
        f'<div class="bar"><span class="l">{inline(label)}</span><div class="v" style="width:{max(0.4, 82 * n / top):.2f}%"><span>{html.escape(value)}</span></div></div>'
        for label, value, n in rows
    )
    return f'<div class="bars">{out}' + (f'<p class="cap">{inline(caption.strip())}</p>' if caption.strip() else "") + "</div>"


def report_of(release: dict) -> dict:
    """The report this release's cards are drawn from, by issue."""
    path = RELEASES / release["version"] / "report" / "report.json"
    if not path.is_file():
        return {}
    import json

    return {i["issue"]: i for i in json.loads(path.read_text())["issues"]}


def card(release: dict, n: int) -> str:
    """An issue as Atlas draws it: its program before and after, and where Atlas's own code changed."""
    issue = report_of(release).get(n)
    if not issue:
        return f'<div class="card"><h3><a href="{ISSUES}{n}">#{n}</a>no picture in the report</h3></div>'
    base = f"../../releases/{release['version']}/report/{n}/"
    builds = {side: html.escape(v.split()[1]) if len(v.split()) > 1 else side for side, v in release.get("builds", {}).items()}
    parts = [f'<div class="card"><h3><a href="{ISSUES}{n}">#{n}</a>{inline(issue.get("title", ""))}</h3>']
    shown = False
    for c in issue["cards"]:
        if c["kind"] == "A" and c.get("pictures") and not shown:
            shown = True
            route = c.get("view", {}).get("after", {}).get("route", c.get("route", ""))
            parts.append(f'<p class="where">{html.escape(c["subject"].removeprefix("engine/tests/"))} · route {html.escape(route)}</p>')
            if c.get("says"):
                parts.append(f"<p>{inline(c['says'])}</p>")
            figures = []
            for side in ("before", "after"):
                if side in c["pictures"]:
                    figures.append(
                        f'<figure><div class="shot"><img src="{base}{c["pictures"][side]}" alt="" loading="lazy"></div>'
                        f'<figcaption><span class="tag {side}">{builds.get(side, side)}</span></figcaption></figure>'
                    )
            parts.append(f'<div class="pair">{"".join(figures)}</div>')
    for c in issue["cards"]:
        if c["kind"] == "C" and c.get("what") == "check" and not shown:
            shown = True
            for pair in c.get("pairs", [])[:2]:
                count = {side: pair[side].get("questions") or 0 for side in ("before", "after")}
                texts = "".join(
                    f'<figure><div class="shot"><pre>{html.escape((RELEASES / release["version"] / "report" / str(n) / pair[side]["text"]).read_text().strip())}</pre></div>'
                    f'<figcaption><span class="tag {side}">{builds.get(side, side)}</span>{count[side]} question{"" if count[side] == 1 else "s"}</figcaption></figure>'
                    for side in ("before", "after")
                    if (RELEASES / release["version"] / "report" / str(n) / pair[side]["text"]).is_file()
                )
                parts.append(f'<p class="where">atlas check {html.escape(pair["program"].removeprefix("engine/tests/"))} · {count["before"]} → {count["after"]} questions</p><div class="pair">{texts}</div>')
    for c in issue["cards"]:
        if c["kind"] == "B":
            routes = [r for r in c.get("routes", []) if r.get("picture")]
            if routes:
                marks = ", ".join(f"<code>{html.escape(f.split('.')[-1])}</code> {m}" for f, m in list(c.get("marked", {}).items())[:6])
                parts.append(
                    f'<details><summary>In Atlas\'s own code · {html.escape(routes[0]["route"])}</summary>'
                    f'<div class="pair"><figure><div class="shot"><img src="{base}{routes[0]["picture"]}" alt="" loading="lazy"></div>'
                    f"<figcaption>{marks}</figcaption></figure></div></details>"
                )
    parts.append("</div>")
    return "".join(parts)


def take(report: pathlib.Path, version: str) -> None:
    """Copy what the notes of `version` name of a report of `tools/release_report.py` into the docs."""
    import json
    import shutil

    release = read(RELEASES / f"{version}.md")
    wanted = {int(n) for p in release["points"] for n in re.findall(r"#(\d+)", p.get("atlas", ""))}
    data = json.loads((report / "report.json").read_text())
    into = RELEASES / version / "report"
    shutil.rmtree(into, ignore_errors=True)
    kept = []
    for issue in data["issues"]:
        if issue.get("issue") not in wanted or "cards" not in issue:
            continue
        names = set()
        for c in issue["cards"]:
            if c["kind"] == "A" and c.get("pictures"):
                names |= set(c["pictures"].values())
                break
        for c in issue["cards"]:
            if c["kind"] == "B":
                names |= {r["picture"] for r in c.get("routes", [])[:1] if r.get("picture")}
        if not any(c["kind"] == "A" and c.get("pictures") for c in issue["cards"]):
            for c in issue["cards"]:
                if c["kind"] == "C" and c.get("what") == "check":
                    names |= {pair[side]["text"] for pair in c.get("pairs", [])[:2] for side in ("before", "after")}
        (into / str(issue["issue"])).mkdir(parents=True, exist_ok=True)
        for name in names:
            shutil.copy(report / str(issue["issue"]) / name, into / str(issue["issue"]) / name)
        kept.append({k: issue[k] for k in ("issue", "title", "cards") if k in issue})
    (into / "report.json").write_text(json.dumps({"builds": data.get("builds", {}), "base": data["base"], "head": data["head"], "issues": kept}, indent=1, ensure_ascii=False) + "\n")
    missing = wanted - {i["issue"] for i in kept}
    print(f"took {len(kept)} issues into {into.relative_to(DOCS.parent)}" + (f"; not in the report: {sorted(missing)}" if missing else ""))


def index(releases: list[dict]) -> str:
    """The page that lists the releases, newest first."""
    def themes(r: dict) -> str:
        return ", ".join(p["title"] for p in r["points"] if not p["title"].startswith(("Smaller", "Not ")))[:160]

    rows = "".join(
        f'<a href="{html.escape(r["version"])}/"><span class="v">{html.escape(r["version"])}</span>'
        f'<span class="t">{inline(themes(r))}</span></a>'
        for r in releases
    )
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Atlas releases</title>
<link rel="icon" href="../logo.svg">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link href="https://fonts.googleapis.com/css2?family=Outfit:wght@400;500;600&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
<style>{STYLE}</style>
</head>
<body>
<main>
<header><img src="../logo.svg" alt=""><span class="name">Atlas</span><nav><a href="../">Atlas</a><a href="https://github.com/raoulg/atlas/releases">Downloads</a></nav></header>
<p class="eyebrow">Releases</p>
<h1>What changed, release by release</h1>
<div class="list">{rows}</div>
</main>
</body>
</html>
"""


def version_key(v: str):
    return tuple(int(n) if n.isdigit() else 0 for n in re.split(r"[.-]", v))


def main(args: list[str]) -> int:
    if args[:1] == ["--take"]:
        take(pathlib.Path(args[1]), args[2])
        args = args[2:]
    sources = sorted(RELEASES.glob("*.md"), key=lambda p: version_key(p.stem), reverse=True)
    if not sources:
        print("no release notes in docs/releases")
        return 1
    releases = [read(p) for p in sources]
    missing = 0
    for release in releases:
        if args and release["version"] not in args:
            continue
        out = RELEASES / release["version"] / "index.html"
        out.parent.mkdir(exist_ok=True)
        out.write_text(page(release))
        print(f"wrote {out.relative_to(DOCS.parent)}")
        for point in release["points"]:
            for picture in [p.strip() for p in point.get("pictures", "").split(",") if p.strip()]:
                if not (DOCS / picture).is_file():
                    print(f"  {release['version']}: '{point['title']}' names {picture}, which is not there")
                    missing += 1
            if point.get("draw"):
                print(f"  {release['version']}: '{point['title']}' has no picture yet: {point['draw']}")
    (RELEASES / "index.html").write_text(index(releases))
    print(f"wrote {(RELEASES / 'index.html').relative_to(DOCS.parent)}")
    return 1 if missing else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
