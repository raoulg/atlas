# Atlas Studio

Atlas Studio is a native Tauri window for a repository's Atlas view and history. It starts the matching `atlas` CLI as a sidecar on loopback and keeps one sidecar per open workspace. The view and history come from `atlas serve` and share its repository index.

## Run locally

Prepare the sidecar, then run the Tauri crate:

```sh
./studio/prepare-sidecar.sh
cargo run --manifest-path studio/src-tauri/Cargo.toml
```

The generated sidecar binary and Tauri build directory are ignored by Git. The sidecar filename includes the Rust host target triple, as required by Tauri.

The bundled viewer contains D3, highlight.js, and ELK and can render without a network connection.

On Windows, run `./studio/prepare-sidecar.ps1` in PowerShell before the Tauri build. The app icon uses the repository's `docs/logo.svg`.

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

The view file is JSON in Atlas's view format, for example `{"route":"demo","open":{"functions":["demo.nearest"]}}`. In Studio, arrange a route in the full Atlas view, then use **Insert view in lesson**. Studio saves the sight in the lesson's `views/` directory and inserts its reference in the lesson file. The embedded view opens the full Atlas view when requested; space in Atlas opens the function's code.

A script can write figures into the lesson's `output/` directory. Markdown uses `![PCA](output/pca.png)`; HTML uses `<img src="output/pca.png" alt="PCA">`. Run the script and the lesson refreshes these images. The script needs no Atlas decorator.

The file remains `.md` or `.html`; no lesson-specific extension is required. **Normal** reads the rendered lesson, and **Insert** edits its source.
