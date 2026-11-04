---
issue: 248
title: A folder is placed as the files in it are
area: engine
commit: 8feac9b0
pictures: [248-folder-before.jpg, 248-folder-after.jpg]
view: {"route": "bench.clear"}  # engine/tests/pairs/workbench/rust/clean
---
A folder whose name the code decides below a folder it leaves to the caller is that name under it, as a file is.
`fs::remove_dir_all(Workbench::of(project).file("report.txt").parent())` removes `{project}/.made/`, the folder
`save_report` writes `{project}/.made/report.txt` into; it was drawn removing `File(report.txt)`, the literal of the
call. The same holds in Python (`shutil.rmtree(... .parent)`) and TypeScript (`rmSync(dirname(...))`), on the clean
and the shortcut side of the workbench pair. The step up `..` names no folder of its own: `{root}/..` rests where
`{root}` does. A folder made for a file is folded into that file when the file lies in it by its term (Atlas's
`Workbench::at` making `{subject}/.atlas` for the files it names there), and a folder made for none of the files
touched is drawn where it is: a test's `$TMPDIR/atlas-shared-{}-look/.git/` was drawn as `$TMPDIR/`, the temporary
directory every test shares, and folded into whatever file another test wrote there.
