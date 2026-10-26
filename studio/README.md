# Atlas Studio

Atlas Studio is a native Tauri window for a repository's Atlas view and history. It starts the matching `atlas` CLI as a sidecar on loopback and keeps one sidecar per open workspace. The view and history come from `atlas serve` and share its repository index.

## Install as an app

`build-app.sh` builds Studio with the matching release `atlas` inside it. On macOS it installs `Atlas Studio.app` into `~/Applications`, where Launchpad, Spotlight and the Dock find it; give another folder as its argument to install there. On Linux and Windows it leaves the platform's packages in `studio/src-tauri/target/release/bundle`. It needs the Tauri CLI:

```sh
cargo install tauri-cli --version "^2" --locked
./studio/build-app.sh
```

Run it again after pulling to install the new build. The app is signed ad hoc on the machine that builds it; packages for other machines, signed and notarized, are #201.

Studio opens the workspace it had open when it last closed. A folder named on its command line opens instead: `open -a "Atlas Studio" --args ~/code/project`. An app started from the Finder or the Dock takes the `PATH` of the user's login shell, so the sidecar, the lessons' Python and the terminal find what the shell profile adds.

## Run from the source

Prepare the debug sidecar, then run the Tauri crate:

```sh
./studio/prepare-sidecar.sh
cargo run --manifest-path studio/src-tauri/Cargo.toml
```

The generated sidecar binary and Tauri build directory are ignored by Git. The sidecar filename includes the Rust host target triple, as required by Tauri. `cargo test --manifest-path studio/src-tauri/Cargo.toml` and `node studio/tests/files.test.js` test the app.

The bundled viewer contains D3, highlight.js, and ELK and can render without a network connection.

On Windows, run `./studio/prepare-sidecar.ps1` in PowerShell before the Tauri build. The app icon uses the repository's `docs/logo.svg`.

## Files

The sidebar shows the workspace's files as Atlas reads them: what `atlas serve` answers at `/api/files`, the listing `Repo` reads, without hidden directories, what git ignores, dependencies and build output. A filled dot marks a file the graph defines something in (a module, class or function); an open dot marks a physical file only. The tree follows the working tree while the workspace is open.

Selecting a file opens it in the preview at the right: source with line numbers, Markdown rendered (with **Source** beside it), HTML in a sandbox without scripts, images and PDF as they are. The preview's footer lists what the graph defines in the file; a name there, or a marked line number, points the Atlas view at its box, opening the way to it. The other way, the box selected in the Atlas view marks its file in the tree with ◇. Arrow keys move through the tree, Right and Left open and close a folder, Enter opens a file.

## Terminal

**Terminal** in the status bar, or Ctrl+` anywhere but inside the Atlas view, opens a terminal under the picture. It runs the user's own login shell in the workspace, on a pseudo-terminal the Rust side owns; + opens another, a tab's × ends its shell, and the top edge drags to resize. Opening another workspace ends the shells of the one before, as quitting Studio ends them all.

## Lessons

Studio lists Markdown and HTML lesson files in the open workspace when they name a Python script. A Markdown lesson starts with:

```markdown
---
atlas-script: demo.py
---
# A lesson
```

An HTML lesson starts with `<!-- atlas-script: demo.py -->`. Both files refer to a script relative to the lesson file. The script is ordinary Python with a `__main__` block. Studio runs the workspace's `.venv` Python when it exists, or `uv run` for that workspace, and shows its stdout and stderr in the lesson panel.

An embedded Atlas view points to a saved sight:

```html
<atlas-view ref="views/predict.json" title="One prediction" />
```

The view file is JSON in Atlas's view format. A sight inside a script route names each opened ancestor step as well as the function it opens, for example `{"route":"demo","open":{"steps":["demo/0"],"functions":["demo.nearest"]}}`. Add `at="demo.nearest"` to the `atlas-view` element to frame that step in the lesson reader. The embedded diagram keeps Atlas drag, zoom, and Space-to-code interaction; **Full view** opens the complete route. In Studio, arrange a route in the full Atlas view, then use **Insert view in lesson**. Studio saves the sight in the lesson's `views/` directory and inserts its reference in the lesson file.

A script can write figures into the lesson's `output/` directory. Markdown uses `![PCA](output/pca.png)`; HTML uses `<img src="output/pca.png" alt="PCA">`. Run the script and the lesson refreshes these images. The script needs no Atlas decorator.

The file remains `.md` or `.html`; no lesson-specific extension is required. **Normal** reads the rendered lesson, and **Insert** edits its source.
