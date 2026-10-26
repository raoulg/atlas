# /// script
# requires-python = ">=3.11"
# ///
"""The release notes of Atlas as pages of the documentation site: one page per stable version, and a
page that lists them.

    uv run tools/release_page.py            # every docs/releases/<version>.md
    uv run tools/release_page.py 0.68.0     # one

A release's notes are `docs/releases/<version>.md`: a head (`version`, `date`, empty until the tag),
a paragraph that says what the release is, and its main points, each a `## ` heading with a few
paragraphs, an `Issues:` line and a `Pictures:` line of files under `docs/` (the notes each issue
leaves in `docs/changes/`, and what `tools/release_report.py` draws). A `Draw:` line says which
picture is still to be drawn for a point; the page leaves it out and this tool names it, so that the
release is not tagged with a point that shows nothing. The page is
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

        rest = re.sub(r"(?ms)^(Issues|Pictures|Draw): (.*?)(?=\n\s*\n|\n(?:Issues|Pictures|Draw): |\Z)", field, rest)
        points.append({"title": re.sub(r"^\d+\.\s*", "", title.strip()), "text": rest, **fields})
    return {"version": head.get("version", path.stem), "date": head.get("date", ""), "lede": parts[0].strip(), "points": points}


def page(release: dict) -> str:
    """The page of one release."""
    v = html.escape(release["version"])
    sections = []
    for point in release["points"]:
        body = blocks(point["text"])
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
{"".join(sections)}
<footer>Made by tools/release_page.py from docs/releases/{v}.md and the notes in docs/changes.</footer>
</main>
</body>
</html>
"""


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
