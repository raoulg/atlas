Route: `ledger.run`. The shortcut side of engine/tests/pairs/readback/python with the class renamed `_Ledger` (not an export).
- DuckDB(notes): between (mid) — what is read back is returned (issue #55 landing comment: "on the shortcut side between the steps").
- Screen: right (out).
Observed (0.68.0): out, the same picture as the clean side. DEFECT A. The Python pair draws "mid" only because `Ledger(...)`, an exported class, reaches its public method `keep` (DEFECT B); in Rust and TypeScript the readback pair's clean and shortcut sides both draw the table "out".
