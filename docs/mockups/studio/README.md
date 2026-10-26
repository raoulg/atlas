# Atlas Studio mockup

Open [index.html](index.html) in a browser. The file is self-contained and uses illustrative content. It shows the proposed desktop layout for #188.

## Layout

- The left side holds the selected workspace's physical file tree and git history. A blue dot means Atlas has a traced relation to that file; an open dot means the file is available to browse without a graph fact.
- The center gives the existing Atlas route view the largest area. A selected box exposes its source, signature, and relation to the file tree. Opening `Language.parse` demonstrates one-family refinement.
- The right side previews a selected Markdown, HTML, PDF, or source file. The Markdown pane has Preview and Source states. The PDF page and HTML content are illustrative renderings.
- The bottom line keeps the active worktree, analysis state, and selected graph element visible.

The `atlas view .` diagram states the intended route: `Disk(repo) → Repo.read → Repo → Language.parse → Graph → (Flow, Findings) → Html → File(html) → open`. It depicts graph structure and source location as different kinds of navigation. A physical directory is not a graph node.

## Interaction to try

1. Select `language.rs` in the tree or `Language.parse` in the graph; the corresponding source and inspector stay in context.
2. Select `README.md` or `atlas-overview.pdf`; the preview changes while the route remains visible. The open dot marks a file without a traced graph relation.
3. Select a history row to change the commit context label. The diagrams in this mockup are illustrative and do not recompute a historical graph.
4. Click `Language.parse` or the plus button to expose its family. The minus button closes it.

## As built

`files.jpg` shows Studio with the file tree as Atlas reads it, a source preview, and the facts the graph defines in the file; `markdown-preview.jpg` a README in the preview. `studio/README.md` describes the app.

## Implementation boundary

The desktop app hosts the current `viewer/index.html`, `viewer/history.html`, and `viewer/plan.js` over the shared workspace index. The center panel in this mockup is a visual placeholder for that viewer, not a second rendering engine. File-tree selection and graph selection share source spans and IDs; previews receive only the selected file's content. HTML previews use an isolated context without Atlas commands or general filesystem access. The native shell, Git actions, and agent harness have separate surfaces and scopes in #197–#200.

The mockup contains no Tauri backend, filesystem access, git operations, PDF renderer, or workspace switching. The `Open workspace` control marks the entry point for a user-selected repository.
