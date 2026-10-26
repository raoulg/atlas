#!/usr/bin/env python3
"""Write the Homebrew cask of Atlas Studio from the two disk images of one build: `atlas-studio` for a
release, `atlas-studio-nightly` for a nightly. The tap jobs of `release.yml` and `nightly.yml` run it
once the build's disk images are published and push the result to the tap, beside the formula.

    tap_cask.py <dist-dir> <version>                       > Casks/atlas-studio.rb
    tap_cask.py <dist-dir> <base-version> <yyyymmddHHMM>   > Casks/atlas-studio-nightly.rb

`<dist-dir>` holds the `atlas-studio-<target>.dmg` files that were uploaded. A release's cask names the
tag `v<version>`; a nightly's names the immutable pre-release `nightly-<yyyymmddHHMM>`, and its version
is the base version and the minute of the build, so that every nightly is newer than the one before it.

Studio is signed ad hoc. Homebrew quarantines what a cask downloads, so macOS blocks the first start
until the user allows it; the caveats say where. Studio installs its own updates (`install-studio.sh`,
which leaves no quarantine), hence `auto_updates`. Standard library only.
"""

from __future__ import annotations

import hashlib
import sys
from pathlib import Path

ARCHES = {"arm": "aarch64", "intel": "x86_64"}
RELEASES = "https://github.com/raoulg/atlas/releases/download"
DMG = "atlas-studio-{arch}-apple-darwin.dmg"
IDENTIFIER = "io.github.raoulg.atlas-studio"

CASK = """cask "{token}" do
  arch arm: "{arm}", intel: "{intel}"

  version "{version}"
  sha256 arm:   "{sha_arm}",
         intel: "{sha_intel}"

  url "{url}"
  name "Atlas Studio"
  desc "{desc}"
  homepage "https://raoulg.github.io/atlas/"
{livecheck}
  auto_updates true
  conflicts_with cask: "{other}"
  depends_on :macos

  app "Atlas Studio.app"

  zap trash: [
    "~/Library/Application Support/{identifier}",
    "~/Library/Caches/{identifier}",
    "~/Library/HTTPStorages/{identifier}",
    "~/Library/Preferences/{identifier}.plist",
    "~/Library/Saved Application State/{identifier}.savedState",
    "~/Library/WebKit/{identifier}",
  ]

  caveats <<~EOS
    Atlas Studio is signed ad hoc, not with an Apple Developer ID, so macOS blocks
    its first start. Open it once, then allow it in
      System Settings > Privacy & Security > Open Anyway

    Studio installs its own updates (Atlas Studio > Check for Updates...), which
    open without asking again. `brew upgrade --greedy` installs a new copy that
    macOS asks about once more.
  EOS
end
"""

LIVECHECK = """
  livecheck do
    url :url
    strategy :github_latest
  end
"""


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def cask(dist: Path, version: str, stamp: str | None = None) -> str:
    """The cask of a release (`stamp` None) or of the nightly built at `stamp`."""
    shas = {cpu: sha256(dist / DMG.format(arch=arch)) for cpu, arch in ARCHES.items()}
    dmg = DMG.format(arch="#{arch}")
    if stamp is None:
        token, other, tag = "atlas-studio", "atlas-studio-nightly", "v#{version}"
        desc, livecheck = "Draws what a program is, beneath the language it is written in", LIVECHECK
    else:
        token, other, tag = "atlas-studio-nightly", "atlas-studio", 'nightly-#{version.split(".").last}'
        version = f"{version}-nightly.{stamp}"
        desc, livecheck = "Draws what a program is, beneath its language: the nightly build", ""
    return CASK.format(
        token=token,
        other=other,
        arm=ARCHES["arm"],
        intel=ARCHES["intel"],
        version=version,
        sha_arm=shas["arm"],
        sha_intel=shas["intel"],
        url=f"{RELEASES}/{tag}/{dmg}",
        desc=desc,
        livecheck=livecheck,
        identifier=IDENTIFIER,
    )


if __name__ == "__main__":
    if len(sys.argv) not in (3, 4):
        sys.exit(__doc__)
    sys.stdout.write(cask(Path(sys.argv[1]), sys.argv[2], sys.argv[3] if len(sys.argv) == 4 else None))
