#!/bin/sh
# Install the latest Atlas Studio for this Mac into $ATLAS_STUDIO_DIR (default ~/Applications).
# ATLAS_CHANNEL=nightly installs the nightly build of `next` instead of the latest release.
#
# Studio is signed ad hoc, not with an Apple Developer ID. A disk image a browser downloads is marked
# as from the internet, and macOS then opens the app only through System Settings › Privacy &
# Security › Open Anyway; what curl downloads carries no such mark, so this opens as it is.
set -eu

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  *) echo "atlas studio: there is a build for macOS only so far; build it from the source:" >&2
     echo "  https://github.com/raoulg/atlas/tree/next/studio#run-from-the-source" >&2
     exit 1 ;;
esac

dir="${ATLAS_STUDIO_DIR:-$HOME/Applications}"
if [ "${ATLAS_CHANNEL:-}" = nightly ]; then from=download/nightly; else from=latest/download; fi
work=$(mktemp -d)
trap 'hdiutil detach -quiet "$work/mount" 2>/dev/null || true; rm -rf "$work"' EXIT
curl -fsSL -o "$work/studio.dmg" "https://github.com/raoulg/atlas/releases/$from/atlas-studio-$target.dmg"
hdiutil attach -quiet -nobrowse -readonly -mountpoint "$work/mount" "$work/studio.dmg"
mkdir -p "$dir"
# a running Studio keeps its files; it runs the new build when it starts again
rm -rf "$dir/Atlas Studio.app"
cp -R "$work/mount/Atlas Studio.app" "$dir/"
echo "Atlas Studio installed in $dir/Atlas Studio.app"
