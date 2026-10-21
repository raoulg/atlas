Route: `manyfiles.run` (seven files, so the viewer folds them into data/)
- File(data/) folded: both — the route uses what was in a1..a4 and leaves b1..b3: as one place, "A place the route reads and then writes on is in both". Opened (dirs), a1..a4 left, b1..b3 right.
Observed (0.68.0): data/ mid (made sure of). DEFECT F: roleOf (viewer/index.html:1989-1996) ORs the members' flags; `found` from a1 and `writes` from b1 read as "written before it is read".
