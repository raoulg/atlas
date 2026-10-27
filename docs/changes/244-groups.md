---
issue: 244
title: "Places fold under the type that builds their path"
area: engine
commit: (this landing)
pictures: [244-groups-before.png, 244-groups-after.png]
view: {"route": "bench.main.main"}  # engine/tests/pairs/workbench/rust/clean
---
A program's type that builds paths keeps the places they name: one box, `Workbench`, stands for the files a
`Workbench` names (`.made/`, `.made/report.txt`, `.made/log.txt`), and a click splits it into them, as a folder splits
into its files. A place is a type's when a value of it holds the path's root (its field), when its constructor takes the
path, or when one of its methods builds the path from more than it is handed; a library's types keep none. The same
files kept through bare paths stand alone, since nothing in that code says they belong together. Atlas drawn by itself
shows its own: `Repo`, `Workbench`, `Cache`, `Tmp`, and the readers' `Libraries`, `Package`, `Setup`.
