#!/bin/sh
# Build the atlas CLI and place it where Tauri finds its sidecar. `--release` builds the optimised
# CLI an installed app carries; without it, the debug CLI for `cargo run`.
set -eu
cd "$(dirname "$0")/.."
profile=debug
if [ "${1:-}" = "--release" ]; then
  profile=release
  cargo build -p atlas --release
else
  cargo build -p atlas
fi
triple=$(rustc --print host-tuple)
mkdir -p studio/src-tauri/binaries
case "$triple" in
  *-windows-*) cp "target/$profile/atlas.exe" "studio/src-tauri/binaries/atlas-$triple.exe" ;;
  *) cp "target/$profile/atlas" "studio/src-tauri/binaries/atlas-$triple" ;;
esac
