#!/bin/sh
# Build Atlas Studio as an installed app, with the matching atlas CLI inside it: on macOS
# `Atlas Studio.app`, copied into ~/Applications (or the folder given as the first argument), where
# Launchpad, Spotlight and the Dock find it; elsewhere the packages Tauri makes for the platform.
# Needs the Tauri CLI: `cargo install tauri-cli --version "^2" --locked`.
set -eu
studio=$(cd "$(dirname "$0")" && pwd)
"$studio/prepare-sidecar.sh" --release
cd "$studio/src-tauri"
# Studio's version is Atlas's own
version=$(grep -m1 '^version' "$studio/../Cargo.toml" | cut -d'"' -f2)
case "$(uname -s)" in
  Darwin)
    cargo tauri build --bundles app --config "{\"version\":\"$version\"}"
    app="$studio/src-tauri/target/release/bundle/macos/Atlas Studio.app"
    into=${1:-"$HOME/Applications"}
    mkdir -p "$into"
    # a running Studio keeps its files; it picks up the new build when it starts again
    rm -rf "$into/Atlas Studio.app"
    cp -R "$app" "$into/"
    echo "Installed $into/Atlas Studio.app"
    ;;
  *)
    cargo tauri build --config "{\"version\":\"$version\"}"
    echo "Packages in $studio/src-tauri/target/release/bundle"
    ;;
esac
