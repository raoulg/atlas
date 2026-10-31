#!/bin/sh
# Install the latest atlas-code binary for this machine into $ATLAS_INSTALL_DIR (default ~/.local/bin).
# ATLAS_CHANNEL=nightly installs the nightly build of `next` instead of the latest release, as
# `atlas-code-nightly` beside the release's `atlas-code`. An `atlas` (or `atlas-nightly`) of Atlas's own
# that an earlier version installed in the same folder is removed: the command is `atlas-code` now.
set -eu

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
  Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-gnu ;;
  *) echo "atlas-code: no prebuilt binary for $(uname -s) $(uname -m); build it with cargo:" >&2
     echo "  cargo install --locked --git https://github.com/raoulg/atlas atlas-code" >&2
     exit 1 ;;
esac

dir="${ATLAS_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$dir"
if [ "${ATLAS_CHANNEL:-}" = nightly ]; then
  from=download/nightly name=atlas-code-nightly old=atlas-nightly
else
  from=latest/download name=atlas-code old=atlas
fi
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
curl -fsSL "https://github.com/raoulg/atlas/releases/$from/atlas-code-$target.tar.gz" | tar -xz -C "$work" atlas-code
mv "$work/atlas-code" "$dir/$name"
echo "atlas-code installed in $dir/$name"
# Atlas's own earlier command says `atlas 0.<minor>...` to --version; another tool's `atlas` stays
if [ -x "$dir/$old" ] && "$dir/$old" --version 2>/dev/null | grep -q '^atlas 0\.'; then
  rm -f "$dir/$old"
  echo "removed the earlier $dir/$old; to keep the short name: alias $old=$name"
fi
case ":$PATH:" in
  *":$dir:"*) ;;
  *) echo "add $dir to your PATH to run it as '$name'" ;;
esac
