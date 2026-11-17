#!/bin/sh
# Install the latest Atlas Studio for this machine, with the atlas of the same release inside it.
# ATLAS_CHANNEL=nightly installs the nightly build of `next` instead of the latest release: an app of
# its own, Atlas Studio Nightly, beside the release.
#
# macOS: the app goes into $ATLAS_STUDIO_DIR (default ~/Applications). Studio is signed ad hoc, not
# with an Apple Developer ID. A disk image a browser downloads is marked as from the internet, and
# macOS then opens the app only through System Settings › Privacy & Security › Open Anyway; what curl
# downloads carries no such mark, so this opens as it is.
#
# Linux (x86_64): the AppImage goes into $ATLAS_STUDIO_DIR (default ~/.local/bin) as `atlas-studio`
# (`atlas-studio-nightly`), with an entry in the application menu. It needs no root. An AppImage runs
# through FUSE 2 (`libfuse2` on Debian and Ubuntu); without it, run it with APPIMAGE_EXTRACT_AND_RUN=1.
# On Debian and Ubuntu the package installs system-wide instead: ATLAS_STUDIO_PACKAGE=deb (uses sudo).
set -eu

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
  *) echo "atlas studio: there is a build for macOS and for Linux on x86_64 (and a setup program for Windows); build it from the source:" >&2
     echo "  https://github.com/raoulg/atlas/tree/next/studio#run-from-the-source" >&2
     exit 1 ;;
esac

if [ "${ATLAS_CHANNEL:-}" = nightly ]; then
  from=download/nightly app="Atlas Studio Nightly" id=atlas-studio-nightly
else
  from=latest/download app="Atlas Studio" id=atlas-studio
fi
base="https://github.com/raoulg/atlas/releases/$from"
work=$(mktemp -d)
trap 'hdiutil detach -quiet "$work/mount" 2>/dev/null || true; rm -rf "$work"' EXIT

case "$target" in
  *-apple-darwin)
    dir="${ATLAS_STUDIO_DIR:-$HOME/Applications}"
    curl -fsSL -o "$work/studio.dmg" "$base/atlas-studio-$target.dmg"
    hdiutil attach -quiet -nobrowse -readonly -mountpoint "$work/mount" "$work/studio.dmg"
    mkdir -p "$dir"
    # a running Studio keeps its files; it runs the new build when it starts again
    rm -rf "${dir:?}/$app.app"
    cp -R "$work/mount/$app.app" "$dir/"
    echo "Atlas Studio installed in $dir/$app.app"
    ;;
  *-linux-gnu)
    if [ "${ATLAS_STUDIO_PACKAGE:-}" = deb ]; then
      curl -fsSL -o "$work/$id.deb" "$base/atlas-studio-$target.deb"
      sudo apt-get install -y "$work/$id.deb"
      echo "$app installed from its package; find it in the application menu"
    else
      dir="${ATLAS_STUDIO_DIR:-$HOME/.local/bin}"
      curl -fsSL -o "$work/$id" "$base/atlas-studio-$target.AppImage"
      chmod +x "$work/$id"
      mkdir -p "$dir"
      # a running Studio keeps its file (the old one is replaced, not overwritten)
      mv -f "$work/$id" "$dir/$id"
      menu="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
      mkdir -p "$menu"
      printf '[Desktop Entry]\nType=Application\nName=%s\nComment=Atlas, a shared language for building software\nExec=%s %%U\nCategories=Development;\nTerminal=false\n' \
        "$app" "$dir/$id" > "$menu/$id.desktop"
      echo "$app installed as $dir/$id, with an entry in the application menu"
      case ":$PATH:" in *":$dir:"*) ;; *) echo "($dir is not in your PATH)" ;; esac
    fi
    ;;
esac
