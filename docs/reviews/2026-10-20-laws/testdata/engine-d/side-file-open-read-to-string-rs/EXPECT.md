Route: `openread.main`
- File(data/log.txt): between (mid) — `load` opens it, reads it into `s` and returns `s`: used.
- Screen: right (out).
Observed (0.68.0): out; the `File::open` touch carries `checks`. DEFECT E in Rust: the handle is used only as a receiver (`f.read_to_string(&mut s)`), which body.rs goes_on does not follow (only arguments count as handed, body.rs:172), nor the out-parameter `s`.
