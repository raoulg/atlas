Route: `paths-hole-env-ts.cli`
- `join(process.env.DATA_DIR ?? "", "sub", "report.txt")`: one place for one read: `*/sub/report.txt` (hole), or with the default "" the relative `sub/report.txt`. Left.
Observed (0.68.0): two places, File(/sub/report.txt) (an absolute path at the file system's root) and Disk({$DATA_DIR}/sub/report.txt|/sub/report.txt). DEFECT J in TypeScript.
