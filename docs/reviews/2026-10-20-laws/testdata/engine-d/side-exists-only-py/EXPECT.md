Route: `existsonly.run`
- File(data/flag): not left — "testing it (whether a file is there ...) puts and takes no content", and left means "the route uses what was there".
- Screen: right (out).
Observed (0.68.0): data/flag in, with a data line flag -> step. DEFECT (medium): a shape touch with no content touch beside it is not `served`, goes to the domain (viewer/index.html:1248-1251), and roleOf falls back to `sideOf` (viewer/index.html:1997).
