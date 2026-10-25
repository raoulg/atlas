#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build -p atlas
triple=$(rustc --print host-tuple)
mkdir -p studio/src-tauri/binaries
case "$triple" in
  *-windows-*) cp target/debug/atlas.exe "studio/src-tauri/binaries/atlas-$triple.exe" ;;
  *) cp target/debug/atlas "studio/src-tauri/binaries/atlas-$triple" ;;
esac
