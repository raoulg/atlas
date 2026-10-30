# Atlas Studio

Atlas Studio is a native Tauri window for a repository's Atlas view and history. It starts the matching `atlas-code` CLI as a sidecar on loopback and keeps one sidecar per open workspace. The view and history come from `atlas-code serve` and share its repository index.

## Install as an app

`build-app.sh` builds Studio with the matching release `atlas-code` inside it. On macOS it installs `Atlas Studio.app` into `~/Applications`, where Launchpad, Spotlight and the Dock find it; give another folder as its argument to install there. On Linux and Windows it leaves the platform's packages in `studio/src-tauri/target/release/bundle`. It needs the Tauri CLI:

```sh
cargo install tauri-cli --version "^2" --locked
./studio/build-app.sh
```

Run it again after pulling to install the new build. Studio's version is Atlas's own, from the root `Cargo.toml`.

Released and nightly builds need no checkout: `brew install --cask raoulg/tap/atlas-studio` (or `atlas-studio-nightly`) installs Studio into `/Applications`, the nightly as an app of its own, Atlas Studio Nightly (`io.github.raoulg.atlas-studio-nightly`), beside the release; `install-studio.sh` installs the disk image of the latest release (or with `ATLAS_CHANNEL=nightly` the nightly) into `~/Applications`, and `install-studio.ps1` runs the Windows setup program for the user alone; see `docs/reference/install.md`. `.github/workflows/studio.yml` makes them, a disk image for Apple silicon and one for Intel and an NSIS setup program for Windows, and `release.yml` and `nightly.yml` publish them; on a push to `next` that changes Studio, or run by hand, it keeps them as the run's artifacts. Neither is signed with a paid identity: the Mac app is signed ad hoc, so a disk image downloaded in a browser, or installed by Homebrew, opens through System Settings › Privacy & Security › Open Anyway, and a setup program downloaded in a browser through SmartScreen's More info › Run anyway.

Studio opens the workspace it had open last. A folder named on its command line opens instead: `open -a "Atlas Studio" --args ~/code/project`. An app started from the Finder or the Dock takes the `PATH` of the user's login shell, so the sidecar, the lessons' Python and the terminal find what the shell profile adds.

## Run from the source

Prepare the debug sidecar, then run the Tauri crate:

```sh
./studio/prepare-sidecar.sh
cargo run --manifest-path studio/src-tauri/Cargo.toml
```

The generated sidecar binary and Tauri build directory are ignored by Git. The sidecar filename includes the Rust host target triple, as required by Tauri. `cargo test --manifest-path studio/src-tauri/Cargo.toml` and `node studio/tests/*.test.js` (each file on its own) test the app.

The bundled viewer contains D3, highlight.js, and ELK and can render without a network connection.

On Windows, run `./studio/prepare-sidecar.ps1` in PowerShell before the Tauri build. The app icon uses the repository's `docs/logo.svg`.

## Updates

Once a day, and again after an update, Studio asks GitHub whether a newer build of its channel is out, as the CLI does: the latest release for a release build, the commit the tag `nightly` is at for a nightly build (and for a build of `next`). Which build Studio is, its own `atlas` says (`atlas-code --version`), which is of the same commit and channel. When one is, a button in the status bar says so, and a page shows what changed: the release's page of notes (`releases/<version>/` on the documentation site), or for a nightly, or a release without a page, a link to the changes on GitHub. The page opens by itself once per newer build. **Update and restart** installs it where Studio is: on macOS through `install-studio.sh` into the app's own folder, then Studio starts again; on Windows the setup program runs, passive, and starts Studio again. Atlas Studio › Check for Updates… (Help › Check for Updates… on Windows and Linux) asks at once. Atlas Studio › What's New in Atlas Studio x.y.z (under Help elsewhere), **What’s new** in Settings, or a click on the version at the right of the status bar, shows the same page for the build that runs: its release's notes, else its release on GitHub, else, for a build that is no release yet, the commits it is made of. A nightly shows the notes of the release it leads to as `next` has them so far (`next/releases/<version>/` on the site, which publishes `next`'s documentation after every nightly), titled What's New in Atlas Studio x.y.z (nightly). `ATLAS_NO_UPDATE_CHECK` set turns the daily look off.

## Workspaces

Studio keeps the last twenty workspaces it opened. ⌘O (File › Open Workspace…) shows them, most recent first: type to filter, arrows and Enter to open one, × to take one off the list. **Open folder…** (⇧⌘O) opens a new one through the system's dialog. File › Open Recent lists the same workspaces, with Clear Recent. With no workspace open, the same page is where Studio starts. The list lives in Studio's configuration folder (`~/Library/Application Support/io.github.raoulg.atlas-studio/workspaces.json` on macOS; the nightly's under `io.github.raoulg.atlas-studio-nightly`).

## Windows

Every window is one workspace, with its own `atlas-code serve` and its own terminals. File › New Window (⌘N) opens one on the page of workspaces; **Open workspace** in the top bar and ⌘O open that page in the window you are in. A workspace already open in another window brings that window to the front, since two servers on one repository would share what it keeps in `.atlas/`. On macOS the windows share a tab group: Window › Merge All Windows makes them tabs. Closing a window ends its server and its shells. The window's title names its workspace, and the status bar names Studio's build.

## Settings

Atlas Studio › Settings… (⌘,; File › Settings… on Windows and Linux):

- **Appearance**: System, Light or Dark, and the theme shown in each: Atlas Light, Catppuccin Latte, GitHub Light, Gruvbox Light or Solarized Light; Atlas Dark, Catppuccin Mocha, Dracula, Gruvbox Dark, Nord or Solarized Dark. Every window follows it, with its terminal and the Atlas view, history and loading page in it. View › Light or Dark (⇧⌘L, Ctrl+Shift+L elsewhere), or ◐ in the status bar, switches between the two themes.

A theme names the roles Atlas's colours go by (`--bg`, `--ink`, `--focus`, the hues, the colours of the kinds of place), from its own published palette (`ui/themes.js`); Studio's colours (`ui/theme.css`) are mixes of those roles. The pages Studio frames know no themes: they take the roles in their address (`palette=`) and, when the theme changes, as a message from Studio's origin, each value checked to be a colour (`viewer/palette.js`).
- **Coding agents**: whether Claude Code and Codex have Atlas's MCP server, registered once for every project as `docs/reference/install.md` says. **Add Atlas** runs that command, with the CLI of Studio's own channel on the `PATH` (`atlas`, or `atlas-code-nightly` for Atlas Studio Nightly), else Studio's own; the agent connects to it in a new session (or `/mcp` in Claude Code).
- **Version**, and a check for updates.

`settings.json` in Studio's configuration folder keeps the appearance, where the terminal docks (`terminal`: `bottom`, `left` or `right`) and where the preview docks (`preview`: `left` or `right`).

## The top bar

In Studio the Atlas view draws no header of its own: its controls (the project, Flowchart / Type lattice / Signature flow, the entry point, what it counts, export, jump to a symbol) stand in Studio's top bar beside the workspace's name. The page opens with `?host=` and Studio's origin; it describes its controls to Studio and does what Studio's bar asks, as messages addressed to that origin only. In a browser the page keeps its header and its link to the history; in Studio the history is in the sidebar (⌘2, with ⌘1 for the Atlas view). **export** saves the page through the system's save dialog.

While Atlas draws, a bar shows how far reading the project's code is; the picture opens when it is done, and a second bar in the picture follows the library calls. Each bar keeps even with the time its phase took at the last few starts of `atlas-code serve` for this repository of the same kind (the slowest of the last three, kept in `.atlas/timings.json`; a bar that expects too little would wait before its end): a start that finds the index of its build of Atlas apart from one that reads everything, as the first start after a new build does. Then it slows down without arriving while the phase takes longer.

## Files

The sidebar shows the workspace's files as Atlas reads them: what `atlas-code serve` answers at `/api/files`, the listing `Repo` reads, without hidden directories, what git ignores, dependencies and build output. A filled dot marks a file the graph defines something in (a module, class or function); an open dot marks a physical file only. The tree follows the working tree while the workspace is open.

A traced file opens where the picture has it: a module that is an entry point (a notebook, a script) on its route, as `atlas-code view notebook.ipynb` does, any other on its module's box. Any other file opens in the preview beside the view, as a traced one does with alt-click, ⇧Enter or the ⧉ on its row. The box selected in the Atlas view marks its file in the tree with ◇. Arrow keys move through the tree, Right and Left open and close a folder, Enter opens a file.

The preview shows:

- source with line numbers, highlighted by highlight.js; a line where something the graph defines starts is marked, and points the Atlas view at its box;
- Markdown rendered by marked, tables, lists and highlighted code included, through the same copy that keeps a lesson's HTML safe (no attribute but a code block's language survives, no script); **Source** shows the text, and a relative link opens the file it names;
- HTML at Studio's own `preview:` address, in a sandbox: the page's images, styles and fonts load from the workspace, no script runs, and nothing loads from the network or from outside the workspace;
- PDF drawn by pdf.js, the same on every platform, a page at a time as it comes into sight;
- images as they are.

The footer lists what the graph defines in the file, each a link to its box, or says that the file has no traced graph fact.

The preview stands where the terminal can: its edge that faces the view drags, as the terminal's does, and the buttons in its head dock it on the left or the right (kept for every window), fold it to a narrow bar at its side that names the open file (a click on the bar unfolds it, its tabs as they were), or let it fill the stage (the same button gives the view back, which keeps its size underneath). A file opened from the tree unfolds a folded preview.

## Terminal

**Terminal** in the status bar, or View › Terminal (Ctrl+`, wherever the focus is), opens a terminal beside the picture. It runs the user's own login shell in the workspace, on a pseudo-terminal the Rust side owns; + opens another, a tab's × ends its shell. The three buttons at the right of its bar dock it under the picture or beside it on the left or the right, in every window, and Studio keeps the choice in its settings; the edge that faces the picture drags to resize. Opening another workspace ends the shells of the one before, as quitting Studio ends them all.

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
