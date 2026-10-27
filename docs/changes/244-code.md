---
issue: 244
title: "Atlas's own places: Repo, Environment, Workbench, Cache, Tmp, Settings"
area: engine
commit: (this landing)
pictures: []
---
Atlas's own code goes to the disk through a handful of types, and Atlas drawn by itself shows them as its places: the
subject (`Repo`, and in Studio the `Workspace` it has open), where the subject's libraries are (`Environment`, for Rust,
TypeScript and Python alike), what Atlas makes beside the subject (`Workbench`), the answers it keeps (`Cache`), a
subject it lays out itself (`Tmp`) and the user's choices (`Settings`). The overview of Atlas's own routes draws, on
the left and the right of its parts:

```text
before (0.69.0 at 1d24ee2)                          after
File({Libraries.locate.found}/package.json)          Repo
File({Libraries.locate.known}/package.json)          Environment
File({Repo.read.root}/{rel}/{rel}/.venv)             Workbench
File({repo}/.venv)  File({root}/..)                  Cache
File({base}/.atlasignore)  File({normal.out}/) · 2   Tmp
Disk(Libraries.locate.from)  Disk(Libraries.root)    Settings
Disk(Notebook.path)  File({ancestor}/.git)  …        Workspace  Check  Html
```

Each opens into the files it keeps. What stands apart is what the code keeps apart: a page written where the command
line says, the executables found on `PATH`, a file the user exports to.
