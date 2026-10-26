"""Extract the page payload (window.DATA) that `atlas view` bakes into its HTML, and print parts of it.

usage: payload.py <page.html> [diff|nodes|edges|flow|all] [filter-substring]
"""
import json
import sys


def payload(path):
    text = open(path, encoding="utf-8").read()
    marker = "window.DATA = "
    i = text.index(marker) + len(marker)
    dec = json.JSONDecoder()
    obj, _ = dec.raw_decode(text[i:])
    return obj


def main():
    path = sys.argv[1]
    what = sys.argv[2] if len(sys.argv) > 2 else "diff"
    filt = sys.argv[3] if len(sys.argv) > 3 else ""
    data = payload(path)
    (name, page), = data["projects"].items()
    if what == "diff":
        d = page.get("diff")
        if d is None:
            print("no diff")
            return
        for k in ["nodes", "renamed", "places", "was", "entries"]:
            v = d.get(k, {})
            print(f"== {k}")
            for kk, vv in sorted(v.items()):
                if filt in kk:
                    print(f"  {kk}: {vv if not isinstance(vv, dict) else json.dumps(vv)}")
        print("== sites (node -> {index: status})")
        for kk, vv in sorted(d.get("sites", {}).items()):
            print(f"  {kk}: {vv}")
        print("== touches (node -> {index: status})")
        for kk, vv in sorted(d.get("touches", {}).items()):
            print(f"  {kk}: {vv}")
        print("== edges (status)")
        for i, st in sorted(d.get("edges", {}).items(), key=lambda x: int(x[0])):
            e = page["edges"][int(i)]
            print(f"  {st}: {e['src']} -{e['kind']}-> {e['dst']} via={e.get('via')}")
    elif what == "nodes":
        for n in page["nodes"]:
            if filt in n["id"]:
                keep = {k: n.get(k) for k in ["id", "kind", "params", "returns", "touches", "instead", "sites", "private", "cls"] if n.get(k) not in (None, [], "")}
                print(json.dumps(keep))
    elif what == "edges":
        for e in page["edges"]:
            if filt in e["src"] or filt in e["dst"]:
                print(f"  {e['src']} -{e['kind']}-> {e['dst']} via={e.get('via')}")
    elif what == "flow":
        print(json.dumps(page["flow"].get(filt) if filt else list(page["flow"].keys()), indent=1)[:20000])
    else:
        print(json.dumps(page, indent=1)[:50000])


if __name__ == "__main__":
    main()
