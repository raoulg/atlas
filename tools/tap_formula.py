#!/usr/bin/env python3
"""Write the Homebrew formulas of Atlas's command line, from the archives of one build: `atlas-code`, of a
release (the tag `v<version>`), and `atlas-code-nightly`, of a nightly build (the immutable pre-release
`nightly-<yyyymmddHHMM>`). The release workflow runs it for the one, the nightly workflow for the other,
and each pushes the result to the tap; a formula names the release it was written from, so its checksums
hold for as long as that release is kept. A nightly's version is the base version and the minute of the
build, so that every nightly is newer than the one before it. The release installs the command
`atlas-code`, the nightly `atlas-code-nightly`, so the two install side by side.

    tap_formula.py <dist-dir> <version>                  > Formula/atlas-code.rb
    tap_formula.py <dist-dir> <base-version> <yyyymmddHHMM> > Formula/atlas-code-nightly.rb

`<dist-dir>` holds the `atlas-code-<target>.tar.gz` files that were uploaded. Standard library only.
"""

from __future__ import annotations

import hashlib
import sys
from pathlib import Path

RELEASES = "https://github.com/raoulg/atlas/releases/download"
ARCHIVE = "atlas-code-{target}.tar.gz"
TARGETS = {
    ("on_macos", "on_arm"): "aarch64-apple-darwin",
    ("on_macos", "on_intel"): "x86_64-apple-darwin",
    ("on_linux", "on_arm"): "aarch64-unknown-linux-gnu",
    ("on_linux", "on_intel"): "x86_64-unknown-linux-gnu",
}

# the version Homebrew reads from the tag in the URLs (`v<version>`)
RELEASE_HEAD = '''class AtlasCode < Formula
  desc "Atlas — your code's topology"
  homepage "https://atlas.pttrn.io/"
  license "AGPL-3.0-only"

  # `brew install --HEAD raoulg/tap/atlas-code` builds the nightly line, `next`, from source.
  head do
    url "https://github.com/raoulg/atlas.git", branch: "next"
    depends_on "rust" => :build
  end
'''

RELEASE_TAIL = '''
  def install
    if build.head?
      ENV["ATLAS_CHANNEL"] = "nightly"
      system "cargo", "install", *std_cargo_args(path: "cli")
    else
      bin.install "atlas-code"
    end
  end

  test do
    assert_match version.to_s.split("-").first, shell_output("#{bin}/atlas-code --version")
  end
end
'''

NIGHTLY_HEAD = '''class AtlasCodeNightly < Formula
  desc "Atlas — your code's topology: the nightly build"
  homepage "https://atlas.pttrn.io/"
  version "{version}"
  license "AGPL-3.0-only"

  # A nightly of `next`, built when `next` has moved and its tests are green: one pre-release per build.
  # It installs `atlas-code-nightly`, beside the release's `atlas-code`.
'''

NIGHTLY_TAIL = '''
  def install
    bin.install "atlas-code" => "atlas-code-nightly"
  end

  test do
    assert_match version.to_s.split("-").first, shell_output("#{bin}/atlas-code-nightly --version")
  end
end
'''


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def archives(dist: Path, tag: str) -> str:
    """The `on_macos`/`on_linux` blocks: each platform's archive of the release `tag`, with its checksum."""
    out = ""
    for os_block in ("on_macos", "on_linux"):
        out += f"\n  {os_block} do\n"
        for cpu in ("on_arm", "on_intel"):
            name = ARCHIVE.format(target=TARGETS[(os_block, cpu)])
            out += f'    {cpu} do\n      url "{RELEASES}/{tag}/{name}"\n'
            out += f'      sha256 "{sha256(dist / name)}"\n    end\n'
        out += "  end\n"
    return out


def formula(dist: Path, version: str, stamp: str | None = None) -> str:
    """The release's formula, or with `stamp` the nightly's of that minute."""
    if stamp is None:
        return RELEASE_HEAD + archives(dist, f"v{version}") + RELEASE_TAIL
    return NIGHTLY_HEAD.format(version=f"{version}-nightly.{stamp}") + archives(dist, f"nightly-{stamp}") + NIGHTLY_TAIL


if __name__ == "__main__":
    if len(sys.argv) not in (3, 4):
        sys.exit(__doc__)
    sys.stdout.write(formula(Path(sys.argv[1]), sys.argv[2], sys.argv[3] if len(sys.argv) == 4 else None))
