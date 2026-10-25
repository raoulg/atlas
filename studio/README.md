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
