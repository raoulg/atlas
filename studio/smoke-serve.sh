#!/bin/sh
# Ask an atlas-code binary for the page Studio's window loads: start `atlas-code serve` the way Studio
# does (a free port, no browser, standard input held open) on a small repository, fetch the page and
# the files it lists. The CLI and the sidecar are the same binary, so a repository that opens to a
# page here opens to that page in the browser and in the window, on every platform.
# Usage: smoke-serve.sh <atlas-code>
set -eu
atlas=$1
work=$(mktemp -d)
trap 'kill "${server:-0}" 2>/dev/null || true; rm -rf "$work"' EXIT
mkdir "$work/repo"
printf '[project]\nname = "smoke"\nversion = "0"\n' > "$work/repo/pyproject.toml"
printf 'def smoke_marker(x: int) -> str:\n    return str(x)\n' > "$work/repo/app.py"
# the server ends when its standard input closes: here after the sleep, at the latest
{ sleep 120 | ATLAS_NO_UPDATE_CHECK=1 NO_COLOR=1 "$atlas" serve "$work/repo" --port 0 --no-open --exit-on-stdin-close; } \
  > "$work/out" 2> "$work/err" &
server=$!
url=
for _ in $(seq 1 60); do
  url=$(grep -o 'http://127.0.0.1:[0-9]*' "$work/err" | head -1 || true)
  [ -n "$url" ] && break
  sleep 1
done
if [ -z "$url" ]; then echo "atlas serve gave no address:" >&2; cat "$work/err" >&2; exit 1; fi
# the server answers with a short waiting page until the repository is read, then with the drawn one
for _ in $(seq 1 90); do
  curl -fsS "$url/" > "$work/page.html" || true
  grep -q '<html' "$work/page.html" && break
  sleep 1
done
if ! grep -q '<html' "$work/page.html"; then
  echo "the page never became the drawn one; the server says:" >&2
  curl -sS "$url/api/drawn" >&2 || true
  echo >&2
  cat "$work/err" >&2
  exit 1
fi
curl -fsS "$url/api/files" > "$work/files.json"
grep -q '"path":"app.py"' "$work/files.json" || { echo "the served files do not name app.py:" >&2; head -c 600 "$work/files.json" >&2; exit 1; }
echo "atlas serve answered at $url: a page of $(wc -c < "$work/page.html" | tr -d ' ') bytes, and app.py in the files it lists"
