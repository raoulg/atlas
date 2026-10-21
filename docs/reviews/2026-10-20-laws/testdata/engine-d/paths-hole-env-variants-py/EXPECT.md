Route: `holevar.run`
- `os.environ["DATA_DIR"] / sub / report.txt`: `*/sub/report.txt` (or `{DATA_DIR}/sub/report.txt`), left.
- `os.environ.get("CACHE_DIR", "cache") / index.json`: one place for one read (the default, or the term with its hole), left.
- `os.getenv("OUT_DIR") / out.csv`: `*/out.csv`, left.
Observed (0.68.0): File(report.txt) (the hole and `sub/` dropped); two places for the one read of index.json (File(cache/index.json) and Disk({$CACHE_DIR}/index.json|cache/index.json)); File(out.csv). DEFECT J.
