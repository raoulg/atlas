Route: `looplog.run`
- File(data/log.txt): between (mid) — the read back is handed to `print` (used): "A place one step writes and a later step reads and uses ... stands between them"; issue #55 decision: "what a step reads from a place is used when it reaches the step's result".
- Screen: right (out).
Observed (0.68.0): out. DEFECT A: the read is dropped by `own_reads_back` before it can count as use (viewer/index.html:1245-1248).
