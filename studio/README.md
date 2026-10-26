# Atlas Studio

Atlas Studio is a native Tauri window for a repository's Atlas view and history. It starts the matching `atlas` CLI as a sidecar on loopback and keeps one sidecar per open workspace. The view and history come from `atlas serve` and share its repository index.

## Install as an app

`build-app.sh` builds Studio with the matching release `atlas` inside it. On macOS it installs `Atlas Studio.app` into `~/Applications`, where Launchpad, Spotlight and the Dock find it; give another folder as its argument to install there. On Linux and Windows it leaves the platform's packages in `studio/src-tauri/target/release/bundle`. It needs the Tauri CLI:

```sh
cargo install tauri-cli --version "^2" --locked
./studio/build-app.sh
```

Run it again after pulling to install the new build. Studio's version is Atlas's own, from the root `Cargo.toml`.

Released and nightly builds need no checkout: `install-studio.sh` installs the disk image of the latest release (or with `ATLAS_CHANNEL=nightly` the nightly) into `~/Applications`, and `install-studio.ps1` runs the Windows setup program for the user alone; see `docs/reference/install.md`. `.github/workflows/studio.yml` makes them, a disk image for Apple silicon and one for Intel and an NSIS setup program for Windows, and `release.yml` and `nightly.yml` publish them; on a push to `next` that changes Studio, or run by hand, it keeps them as the run's artifacts. Neither is signed with a paid identity: the Mac app is signed ad hoc, so a disk image downloaded in a browser opens through System Settings › Privacy & Security › Open Anyway, and a setup program downloaded in a browser through SmartScreen's More info › Run anyway.

Studio opens the workspace it had open last. A folder named on its command line opens instead: `open -a "Atlas Studio" --args ~/code/project`. An app started from the Finder or the Dock takes the `PATH` of the user's login shell, so the sidecar, the lessons' Python and the terminal find what the shell profile adds.

## Run from the source

Prepare the debug sidecar, then run the Tauri crate:

```sh
./studio/prepare-sidecar.sh
cargo run --manifest-path studio/src-tauri/Cargo.toml
```

The generated sidecar binary and Tauri build directory are ignored by Git. The sidecar filename includes the Rust host target triple, as required by Tauri. `cargo test --manifest-path studio/src-tauri/Cargo.toml` and `node studio/tests/files.test.js` test the app.

The bundled viewer contains D3, highlight.js, and ELK and can render without a network connection.

On Windows, run `./studio/prepare-sidecar.ps1` in PowerShell before the Tauri build. The app icon uses the repository's `docs/logo.svg`.

## Updates

Once a day Studio asks GitHub whether a newer build of its channel is out, as the CLI does: a release for a release build, the newest nightly for a nightly build (and for a build of `next`). Which build Studio is, its own `atlas` says (`atlas --version`), which is of the same commit and channel. When one is, a button in the status bar says so, and a page shows what changed: the release's page of notes (`releases/<version>/` on the documentation site), or for a nightly, or a release without a page, a link to the changes on GitHub. The page opens by itself once per newer build. **Update and restart** installs it where Studio is: on macOS through `install-studio.sh` into the app's own folder, then Studio starts again; on Windows the setup program runs, passive, and starts Studio again. Atlas Studio › Check for Updates… (Help › Check for Updates… on Windows and Linux) asks at once. `ATLAS_NO_UPDATE_CHECK` set turns the daily look off.

## Workspaces

Studio keeps the last twenty workspaces it opened. ⌘O (File › Open Workspace…) shows them, most recent first: type to filter, arrows and Enter to open one, × to take one off the list. **Open folder…** (⇧⌘O) opens a new one through the system's dialog. File › Open Recent lists the same workspaces, with Clear Recent. With no workspace open, the same page is where Studio starts. The list lives in Studio's configuration folder (`~/Library/Application Support/io.github.raoulg.atlas-studio/workspaces.json` on macOS).

## The top bar

In Studio the Atlas view draws no header of its own: its controls (the project, Flowchart / Type lattice / Signature flow, the entry point, what it counts, export, jump to a symbol) stand in Studio's top bar beside the workspace's name. The page opens with `?host=` and Studio's origin; it describes its controls to Studio and does what Studio's bar asks, as messages addressed to that origin only. In a browser the page keeps its header and its link to the history; in Studio the history is in the sidebar (⌘2, with ⌘1 for the Atlas view). **export** saves the page through the system's save dialog.

While Atlas draws, a bar shows how far reading the project's code is; the picture opens when it is done, and a second bar in the picture follows the library calls. Each bar keeps even with the time its phase took at the last few starts of `atlas serve` for this repository (their median, kept in `.atlas/timings.json`), then slows down without arriving while the phase takes longer.

## Files

The sidebar shows the workspace's files as Atlas reads them: what `atlas serve` answers at `/api/files`, the listing `Repo` reads, without hidden directories, what git ignores, dependencies and build output. A filled dot marks a file the graph defines something in (a module, class or function); an open dot marks a physical file only. The tree follows the working tree while the workspace is open.

A traced file opens where the picture has it: a module that is an entry point (a notebook, a script) on its route, as `atlas view notebook.ipynb` does, any other on its module's box. Any other file opens in the preview at the right, as a traced one does with alt-click, ⇧Enter or the ⧉ on its row. The box selected in the Atlas view marks its file in the tree with ◇. Arrow keys move through the tree, Right and Left open and close a folder, Enter opens a file.

The preview shows:

- source with line numbers, highlighted by highlight.js; a line where something the graph defines starts is marked, and points the Atlas view at its box;
- Markdown rendered by marked, tables, lists and highlighted code included, through the same copy that keeps a lesson's HTML safe (no attribute but a code block's language survives, no script); **Source** shows the text, and a relative link opens the file it names;
- HTML at Studio's own `preview:` address, in a sandbox: the page's images, styles and fonts load from the workspace, no script runs, and nothing loads from the network or from outside the workspace;
- PDF drawn by pdf.js, the same on every platform, a page at a time as it comes into sight;
- images as they are.

The footer lists what the graph defines in the file, each a link to its box, or says that the file has no traced graph fact.

## Terminal

**Terminal** in the status bar, or View › Terminal (Ctrl+`, wherever the focus is), opens a terminal under the picture. It runs the user's own login shell in the workspace, on a pseudo-terminal the Rust side owns; + opens another, a tab's × ends its shell, and the top edge drags to resize. Opening another workspace ends the shells of the one before, as quitting Studio ends them all.

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
