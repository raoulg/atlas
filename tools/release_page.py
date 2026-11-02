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
- `Diagram: releases/<version>/x.svg | caption; releases/<version>/y.svg`: drawings of the point's
  structure, inline, so that they take the page's colours in the light and the dark, each with what it
  shows beneath it.
- `Bars: label = 18.9 s; label = 0.14 s | caption`: a bar chart of measured numbers.
- `Bench: releases/<version>/bench.json | caption`: the release's speed, the same measures every release
  (`tools/perf.py release-bars` writes the file): its one number, the geometric mean of the ratios after / before;
  a line per measure, its mean over the programs on a log scale from twice as fast to twice as slow, a dot and a
  change per program, and the measures no program changed beyond noise named in one sentence; folded beneath, per
  program of the corpus, named by its card, each measure of the release before beside this one, and each measure
  over every release since the series began, a bar per release.
- `Pictures: changes/x.jpg`: screenshots, from the notes each issue leaves in `docs/changes/`.
- `Example: name | code`: the small program `fixtures/<name>` drawn by the release before and by this one, beneath
  the code the point is about; `Example: name` shows the program's own code, `Example: name |` the pictures alone.

A `Draw:` line says which picture is still to be made for a point; the page leaves it out and this
tool names it, so that the release is not tagged with a point that shows nothing. The page is
`docs/releases/<version>/index.html`, in the site's own style, and ends in links to the release before
and the release after (writing one page writes its neighbours too); `docs/releases/index.html` lists the
releases, newest first. Pages publishes `docs/` from `main`, so a page goes live with the promotion
that carries it, at `https://atlas.pttrn.io/releases/<version>/`; until then the site's copy of `next`'s
documentation has it, at `https://atlas.pttrn.io/next/releases/<version>/`, which a nightly Studio shows.
"""

import html
import json
import math
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
.b-legend{display:flex;flex-wrap:wrap;gap:6px 18px;font-size:13px;color:var(--ink-2);margin:0 0 10px}
.b-legend i{display:inline-block;width:12px;height:12px;margin-right:6px;vertical-align:-1px}
.gbar{display:grid;grid-template-columns:minmax(120px,30%) 1fr;gap:12px;align-items:center;margin:10px 0}
.gbar .l{font-size:14px;color:var(--ink-2)}
.gpair{display:grid;gap:3px}
.gbar:has(> .l:empty){grid-template-columns:1fr}
.gpair .v{height:14px;min-width:2px;position:relative}
.gpair .v.none{min-width:0}
.gpair .v span{position:absolute;left:calc(100% + 8px);top:-1px;font-family:var(--mono);font-size:12.5px;color:var(--ink);white-space:nowrap}
.s0{background:var(--ink-3)}.s1{background:var(--key)}.s2{background:var(--edit)}.s3{background:var(--add)}
.bench{margin:22px 0;background:var(--panel);border:1px solid var(--line);padding:16px 18px}
.b-row{border-top:1px solid var(--line);padding:14px 0 6px}
.b-row:first-child{border-top:0;padding-top:0}
.b-name{margin:0;font-weight:500;color:var(--ink)}
.b-card{margin:2px 0 10px;font-family:var(--mono);font-size:12px;color:var(--ink-3)}
.b-cells{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,220px),1fr));gap:12px 22px}
.b-cell .l{font-size:13px;color:var(--ink-2);margin:0 0 4px}
.b-bar{display:grid;grid-template-columns:56px 1fr;gap:8px;align-items:center;font-family:var(--mono);font-size:12px;color:var(--ink-3);margin:3px 0}
.b-bar .v{height:14px;min-width:2px;position:relative}.b-bar .v.none{min-width:0}
.b-bar .v span{position:absolute;left:calc(100% + 6px);top:-1px;white-space:nowrap;color:var(--ink)}
.b-bar.before .v{background:var(--ink-3)}.b-bar.after .v{background:var(--key)}
.b-ratio{font-family:var(--mono);font-size:12px;color:var(--ink-3);margin:2px 0 0}
.b-lines{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,260px),1fr));gap:14px 22px}
.b-lines figure{margin:0}
.b-lines figcaption{font-size:12.5px;color:var(--ink-2);margin:0 0 2px}
.b-head{border-top:1px solid var(--line);padding-top:14px;margin:18px 0 10px;font-weight:500;color:var(--ink)}
.b-sum{display:flex;gap:16px;align-items:baseline;flex-wrap:wrap;margin:0 0 12px}
.b-sum span{font-size:14px;color:var(--ink-2);max-width:60ch}
.b-sum .b-big{font-family:var(--mono);font-size:30px;color:var(--ink);max-width:none}
.b-scale{font-family:var(--mono);font-size:11.5px;color:var(--ink-3);margin:0 0 4px}
.b-scale .t{display:flex;justify-content:space-between;gap:6px;height:auto;white-space:nowrap}
.b-who{margin:10px 0 2px;font-size:13px;font-weight:500;color:var(--ink)}
.b-div{display:grid;grid-template-columns:min(36%,240px) 1fr 64px;gap:10px;align-items:center;margin:3px 0;font-size:12.5px}
.b-div .l{color:var(--ink-2)}
.b-div .t{position:relative;height:12px}
.b-div .m{position:absolute;left:50%;top:-3px;bottom:-3px;border-left:1px solid var(--ink-3)}
.b-div .d{position:absolute;top:0;height:12px;min-width:1px}
.b-div .d.faster{background:var(--add)}.b-div .d.slower{background:var(--del)}.b-div .d.same{background:var(--line)}
.b-div .n{font-family:var(--mono);font-size:12px;white-space:nowrap}
.b-div .w{grid-column:1/-1;font-size:11.5px;color:var(--ink-3);margin:-2px 0 8px}
.b-div .w b{font-weight:400;white-space:nowrap}.b-div .w b.faster{color:var(--add)}.b-div .w b.slower{color:var(--del)}
.b-div .p{position:absolute;top:1px;width:9px;height:9px;margin-left:-4.5px;border-radius:50%;border:1.5px solid var(--panel)}
.b-div .p.faster{background:var(--add)}.b-div .p.slower{background:var(--del)}.b-div .p.same{background:var(--ink-3)}
.b-div .d{opacity:.45}
.b-kept{font-size:13px;color:var(--ink-3);margin:10px 0 0}
.b-more{border-top:1px solid var(--line);margin-top:14px;padding-top:10px}
.b-more summary{cursor:pointer;font-weight:500;color:var(--ink);margin-bottom:8px}
.b-div .n.faster{color:var(--add)}.b-div .n.slower{color:var(--del)}.b-div .n.same{color:var(--ink-3)}
.bench .cap{font-size:13.5px;color:var(--ink-2);margin:14px 0 0}
.pager{display:flex;justify-content:space-between;gap:16px;border-top:1px solid var(--line);margin-top:48px;padding-top:20px;font-family:var(--mono);font-size:14px}
.pager a{text-decoration:none;color:var(--ink-2)}
.pager a:hover{color:var(--accent)}
.pager .newer{margin-left:auto}
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

        keys = "Issues|Pictures|Draw|Atlas|Diagram|Bars|Bench|Hero|Example|Compare"
        rest = re.sub(rf"(?ms)^({keys}): (.*?)(?=\n\s*\n|\n(?:{keys}): |\Z)", field, rest)
        points.append({"title": re.sub(r"^\d+\.\s*", "", title.strip()), "text": rest, **fields})
    release = {"version": head.get("version", path.stem), "date": head.get("date", ""), "lede": parts[0].strip(), "points": points}
    report = RELEASES / release["version"] / "report" / "report.json"
    if report.is_file():
        import json

        release["builds"] = json.loads(report.read_text()).get("builds", {})
    release.setdefault("builds", {"before": f"atlas {head.get('before', 'before')}", "after": f"atlas {release['version']}"})
    return release


def pager(older: dict | None, newer: dict | None) -> str:
    """The links to the release before and the release after."""
    links = []
    if older:
        links.append(f'<a class="older" href="../{html.escape(older["version"])}/">← Atlas {html.escape(older["version"])}</a>')
    if newer:
        links.append(f'<a class="newer" href="../{html.escape(newer["version"])}/">Atlas {html.escape(newer["version"])} →</a>')
    return f'<nav class="pager">{"".join(links)}</nav>' if links else ""


def page(release: dict, older: dict | None = None, newer: dict | None = None) -> str:
    """The page of one release, with links to its neighbours."""
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
        for drawing in [d for d in point.get("diagram", "").split(";") if d.strip()]:
            diagram, _, caption = drawing.partition("|")
            said = f"<figcaption>{inline(caption.strip())}</figcaption>" if caption.strip() else ""
            body.append(f'<figure class="diagram">{svg(DOCS / diagram.strip())}{said}</figure>')
        if point.get("bars"):
            body.append(bars(point["bars"]))
        if point.get("bench"):
            body.append(bench(point["bench"]))
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
{pager(older, newer)}
<footer>Made by tools/release_page.py from docs/releases/{v}.md and the notes in docs/changes.</footer>
</main>
</body>
</html>
"""


def example(release: dict, spec: str) -> str:
    """A small program of `fixtures/<name>` (fixtures: Atlas reads no part there, so the program stays out
    of Atlas's own picture) and the picture of it by the release before and by this one
    (`tools/release_example.py` draws them into `img/<name>-before.png` and `-after.png`), beneath the code
    after `|`: the program's own code where the spec names none, nothing where it is empty."""
    name, bar, shown = (x.strip() for x in spec.partition("|"))
    folder = RELEASES / release["version"] / "fixtures" / name
    source = next((p for p in sorted(folder.rglob("*.py")) if "typings" not in p.parts), None)
    code = shown if bar else source.read_text().split("\n\n\ndef main(")[0].strip() if source else ""
    builds = {side: html.escape(v.split()[1]) if len(v.split()) > 1 else side for side, v in release.get("builds", {}).items()}
    labels = {"before": builds.get("before", "before"), "after": builds.get("after", release["version"])}
    figures = "".join(
        f'<figure><div class="shot"><img src="img/{html.escape(name)}-{side}.png" alt="" loading="lazy"></div>'
        f'<figcaption><span class="tag {side}">{labels[side]}</span></figcaption></figure>'
        for side in ("before", "after")
    )
    listing = f"<pre><code>{html.escape(code)}</code></pre>" if code else ""
    return f'<div class="example">{listing}<div class="pair">{figures}</div></div>'


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


SIDE = re.compile(r"^(.*?)(?:,\s*|\s+|^)(before|after|now|v?\d+\.\d+(?:\.\d+)?)$", re.IGNORECASE)


def sides_of(labels: list[str]) -> list[tuple[str, str]] | None:
    """Labels that compare versions, `measure, 0.70.0` / `measure, 0.71.0` or `measure before` / `measure after`, as
    (measure, side); None where any label names no side or there is one side alone: then each label is a bar of its own."""
    split = [SIDE.match(label.strip()) for label in labels]
    if not all(split) or len({m.group(2) for m in split}) < 2:
        return None
    return [(m.group(1).strip(), m.group(2)) for m in split]


def legend(sides: list[str]) -> str:
    """The colour of each side of a comparison, in the order of the pairs."""
    keys = "".join(f'<span><i class="s{i}"></i>{inline(side)}</span>' for i, side in enumerate(sides))
    return f'<p class="b-legend">{keys}</p>'


def pairs(groups: list[tuple[str, list[tuple[int, str, float | None]]]], top: float, longest: float = 78) -> str:
    """Grouped bars of one scale: a group per measure (its label as HTML), its sides side by side, each in its side's
    colour; the longest bar takes `longest` percent of the width, the rest is room for its number."""
    out = []
    for measure, members in groups:
        rows = "".join(
            f'<div class="v s{i}{" none" if n is None else ""}" style="width:{0 if n is None else max(0.4, longest * n / top):.2f}%"><span>{html.escape(said)}</span></div>'
            for i, said, n in members
        )
        out.append(f'<div class="gbar"><span class="l">{measure}</span><div class="gpair">{rows}</div></div>')
    return "".join(out)


def bars(spec: str) -> str:
    """`label = 18.9 s; label = 0.14 s | caption` as bars of one scale. Labels that compare versions or before and
    after (`sides_of`) are drawn as pairs: a group per measure, one colour per side, with a legend."""
    data, _, caption = spec.partition("|")
    rows = []
    for item in [i for i in data.split(";") if "=" in i]:
        label, value = (x.strip() for x in item.split("=", 1))
        number = float(re.match(r"[\d.]+", value).group(0))
        rows.append((label, value, number))
    top = max(n for _, _, n in rows) or 1
    said = f'<p class="cap">{inline(caption.strip())}</p>' if caption.strip() else ""
    split = sides_of([label for label, _, _ in rows])
    if split:
        sides = list(dict.fromkeys(side for _, side in split))
        groups: dict[str, list[tuple[int, str, float | None]]] = {}
        for (measure, side), (_, value, number) in zip(split, rows):
            groups.setdefault(measure, []).append((sides.index(side), value, number))
        return f'<div class="bars">{legend(sides)}{pairs([(inline(m), g) for m, g in groups.items()], top)}{said}</div>'
    out = "".join(
        f'<div class="bar"><span class="l">{inline(label)}</span><div class="v" style="width:{max(0.4, 82 * n / top):.2f}%"><span>{html.escape(value)}</span></div></div>'
        for label, value, n in rows
    )
    return f'<div class="bars">{out}{said}</div>'


def measured(value: float | None, unit: str) -> str:
    """A measured number as the page says it."""
    if value is None:
        return "–"
    if unit == "MB":
        return f"{value:,.0f} MB"
    return f"{value:.2f} s" if value < 1 else f"{value:.1f} s" if value < 100 else f"{value:,.0f} s"


def change(ratio: float) -> str:
    """A ratio after / before as a change: ×1.25 is `+25 %`, ×0.986 is `−1.4 %`."""
    percent = (ratio - 1) * 100
    said = f"{abs(percent):.1f}" if abs(percent) < 10 else f"{abs(percent):.0f}"
    return f"{'+' if percent > 0 else '−' if percent < 0 else '±'}{said} %"


def bench(spec: str) -> str:
    """`releases/<version>/bench.json | caption`: the release's speed. First its one number, the geometric mean of
    after / before over every measure, and a line per measure across the programs (`summary`); then, folded, a row per
    program, named by its card (a private one by its card alone), each measure as two bars, the release before and this
    one; then, folded, each measure on the night of every release since the series began, a bar per release."""
    path, _, caption = (x.strip() for x in spec.partition("|"))
    data = json.loads((DOCS / path).read_text())
    before, after = html.escape(data["before"]), html.escape(data["after"])
    parts, rows = [summary(data)], []
    for row in data["rows"]:
        groups = []
        for c in row["cells"]:
            values = [v for v in (c["before"], c["after"]) if v is not None]
            ratio = (
                ("to 0" if c["after"] == 0 else f"×{c['after'] / c['before']:.2f}")
                if c["before"] and c["after"] is not None
                else "not measured on both"
            )
            groups.append((f"{html.escape(c['label'])} <span class=\"b-ratio\">{ratio}</span>", c, max(values or [0]) or 1))
        cells = "".join(
            pairs([(label, [(0, measured(c["before"], c["unit"]), c["before"]), (1, measured(c["after"], c["unit"]), c["after"])])], peak)
            for label, c, peak in groups
        )
        name = f"<code>{html.escape(row['name'])}</code>" if row.get("name") else html.escape(row.get("title") or "A private program")
        said = f'<p class="b-card">{html.escape(row["said"])}</p>' if row.get("said") else ""
        rows.append(f'<div class="b-row"><p class="b-name">{name}</p>{said}{cells}</div>')
    parts.append(f'<details class="b-more"><summary>Every measure, per program</summary>{legend([before, after])}{"".join(rows)}</details>')
    over = "".join(releases_bars(data["releases"], s, data["rows"][s["row"]].get("short", "")) for s in data["series"])
    if over:
        parts.append(f'<details class="b-more"><summary>Every release since the series began, on its night</summary><div class="b-lines">{over}</div></details>')
    if caption:
        parts.append(f'<p class="cap">{inline(caption)}</p>')
    return f'<div class="bench">{"".join(parts)}</div>'


# The ratio chart's edge: twice as fast and twice as slow, the same in every release; a ratio beyond it stands at the edge.
SCALE = 2
LANGUAGE = {"python": "Python", "rust": "Rust", "typescript": "TypeScript"}

def summary(data: dict) -> str:
    """The release's one number, then a line per measure: its bar the geometric mean of after / before over the
    programs, on a log scale (twice as fast and twice as slow are bars of one length, on either side of the middle),
    a dot per program where its own ratio lies, and beneath it which programs changed beyond noise. A measure no
    program changed beyond noise draws no line: the measures every program kept are named together. A ratio that
    fell to 0 has no finite log: its dot stands at the faster edge, said as `to 0`, and it stays out of the mean."""
    s = data.get("summary")
    if not s:
        return ""
    measures: dict[str, list[tuple[str, float, bool]]] = {}
    for row in data["rows"]:
        who = (row.get("short") or row.get("name") or "").split(",")[0]
        tongues = (row.get("card") or {}).get("languages") or []
        if tongues and row.get("name"):
            who += f" ({' + '.join(LANGUAGE.get(x, x) for x in tongues)})"
        for c in row["cells"]:
            if c["before"] and c["after"] is not None:
                measures.setdefault(c["label"], []).append((who, c["after"] / c["before"], bool(c.get("changed"))))
    top = math.log(SCALE)

    def at(r: float) -> float:
        return 50 + 48 * max(-1, min(1, math.log(r) / top)) if r else 2

    lines, kept = [], []
    for label, ms in measures.items():
        if not any(changed for _, _, changed in ms):
            kept.append(label)
            continue
        finite = [r for _, r, _ in ms if r]
        mean = math.exp(sum(math.log(r) for r in finite) / len(finite)) if finite else 0
        side = "faster" if mean < 1 else "slower"
        width = abs(at(mean) - 50)
        place = f"right:50%;width:{width:.1f}%" if mean < 1 else f"left:50%;width:{width:.1f}%"
        dots = "".join(
            f'<i class="p {("faster" if r < 1 else "slower") if changed else "same"}" style="left:{at(r):.1f}%" '
            f'title="{html.escape(who)}: {change(r) if r else "to 0"}"></i>'
            for who, r, changed in ms
        )
        said = " · ".join(
            f'<b class="{("faster" if r < 1 else "slower") if changed else "same"}">{html.escape(who)} {change(r) if r else "to 0"}</b>'
            for who, r, changed in sorted(ms, key=lambda m: m[1])
        )
        lines.append(
            f'<div class="b-div"><span class="l">{html.escape(label)}</span>'
            f'<div class="t"><i class="m"></i><i class="d {side}" style="{place}"></i>{dots}</div>'
            f'<span class="n {side}">{change(mean) if mean else "to 0"}</span><small class="w">{said}</small></div>'
        )
    word = "faster" if s["ratio"] < 1 else "slower"
    gone = s.get("zero", 0)
    zero = f" {gone} more fell to 0, work the release no longer does, and stand outside the mean." if gone else ""
    head = (
        f'<div class="b-sum"><span class="b-big">{change(s["ratio"])}</span><span>{html.escape(data["after"])} against '
        f'{html.escape(data["before"])}: the geometric mean of after / before over {s["measures"]} measures, '
        f'{abs(s["ratio"] - 1) * 100:.1f} % {word}; {s["changed"]} of them changed beyond noise.{zero}</span></div>'
    )
    legend = (
        '<div class="b-div b-scale"><span class="l">bar: the mean over the programs; dot: one program, grey within noise</span>'
        f'<div class="t"><span>← ×1/{SCALE:g} faster</span><span>slower ×{SCALE:g} →</span></div><span></span></div>'
    )
    same = f'<p class="b-kept">Within noise on every program: {html.escape(", ".join(kept))}.</p>' if kept else ""
    return f'<div class="b-row">{head}{legend}{"".join(lines)}{same}</div>'


def releases_bars(releases: list[str], series: dict, short: str) -> str:
    """One measure over the releases, as grouped bars are drawn: the releases side by side from zero, the newest in
    this release's colour and the ones before in the colour of the release before."""
    shown = {p[0]: p for p in series["points"]}
    top = max(p[1] for p in series["points"]) or 1
    last = len(releases) - 1
    members = [
        (1 if i == last else 0, f"{r}: {measured(shown[r][1], series['unit'])}" if r in shown else f"{r}: –", shown[r][1] if r in shown else None)
        for i, r in enumerate(releases)
    ]
    title = f"{html.escape(short)}, {html.escape(series['label'])}" if short else html.escape(series["label"])
    return f"<figure><figcaption>{title}</figcaption>{pairs([('', members)], top, longest=45)}</figure>"


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
                parts.append(f'<p class="where">atlas-code check {html.escape(pair["program"].removeprefix("engine/tests/"))} · {count["before"]} → {count["after"]} questions</p><div class="pair">{texts}</div>')
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
    # newest first: a release's neighbours link to it, so a page asked for is written with theirs
    asked = {i for i, r in enumerate(releases) if not args or r["version"] in args}
    written = {j for i in asked for j in (i - 1, i, i + 1) if 0 <= j < len(releases)}
    for i, release in enumerate(releases):
        if i not in written:
            continue
        newer = releases[i - 1] if i > 0 else None
        older = releases[i + 1] if i + 1 < len(releases) else None
        out = RELEASES / release["version"] / "index.html"
        out.parent.mkdir(exist_ok=True)
        out.write_text(page(release, older, newer))
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
