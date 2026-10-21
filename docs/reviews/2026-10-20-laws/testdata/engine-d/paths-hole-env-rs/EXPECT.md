Route: `holeenv.main`
- `PathBuf::from(env::var("DATA_DIR")).join("sub").join("report.txt")`: `*/sub/report.txt` (a named hole), left.
Observed (0.68.0): File(report.txt). DEFECT J in Rust.
