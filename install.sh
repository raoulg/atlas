#!/bin/sh
# Install the latest atlas binary for this machine into $ATLAS_INSTALL_DIR (default ~/.local/bin).
# ATLAS_CHANNEL=nightly installs the nightly build of `next` instead of the latest release, as
# `atlas-nightly` beside the release's `atlas`.
set -eu

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
  Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-gnu ;;
  *) echo "atlas: no prebuilt binary for $(uname -s) $(uname -m); build it with cargo:" >&2
     echo "  cargo install --locked --git https://github.com/raoulg/atlas atlas" >&2
     exit 1 ;;
esac

dir="${ATLAS_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$dir"
if [ "${ATLAS_CHANNEL:-}" = nightly ]; then from=download/nightly name=atlas-nightly; else from=latest/download name=atlas; fi
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
curl -fsSL "https://github.com/raoulg/atlas/releases/$from/atlas-$target.tar.gz" | tar -xz -C "$work" atlas
mv "$work/atlas" "$dir/$name"
echo "atlas installed in $dir/$name"
case ":$PATH:" in
  *":$dir:"*) ;;
  *) echo "add $dir to your PATH to run it as '$name'" ;;
esac
