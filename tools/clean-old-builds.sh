#!/bin/sh
# Keeps target/ of the worktree it is run in to what recent builds use, when asked: it is run by hand,
# never by a git hook, so it never removes a build while one is made or used. Cargo names what it
# builds by the package's version and by the hash of its dependencies and toolchain, so a raised
# version, an updated dependency or a new Rust each leave a full set of builds behind. cargo-sweep
# (from the Brewfile) removes the builds of a toolchain no longer installed, then the least recently
# used builds until target/ is below 15 GB: an earlier version's are those, and today's builds stay.
set -eu
[ -d target ] || { echo "no target/ here"; exit 0; }
command -v cargo-sweep >/dev/null || { echo "cargo-sweep is not installed (Brewfile)"; exit 1; }
cargo sweep --installed
cargo sweep --maxsize 15GB
du -sh target
