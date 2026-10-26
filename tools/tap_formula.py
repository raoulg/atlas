#!/usr/bin/env python3
"""Write the Homebrew formula of the nightly build: `atlas-code-nightly`, from the binaries of one
nightly build. The nightly workflow runs it after it publishes the immutable pre-release
`nightly-<yyyymmddHHMM>` and pushes the result to the tap; the formula names that pre-release, so
its checksums hold for as long as it is kept. Its version is the base version and the minute of
the build, so that every nightly is newer than the one before it. It installs the command as
`atlas-nightly`, beside the release's `atlas` (`atlas-code`).

    tap_formula.py <dist-dir> <base-version> <yyyymmddHHMM> > Formula/atlas-code-nightly.rb

`<dist-dir>` holds the `atlas-<target>.tar.gz` files that were uploaded. Standard library only.
"""

import hashlib
import sys
from pathlib import Path

URL = "https://github.com/raoulg/atlas/releases/download/nightly-{stamp}/atlas-{target}.tar.gz"
TARGETS = {
    ("on_macos", "on_arm"): "aarch64-apple-darwin",
    ("on_macos", "on_intel"): "x86_64-apple-darwin",
    ("on_linux", "on_arm"): "aarch64-unknown-linux-gnu",
    ("on_linux", "on_intel"): "x86_64-unknown-linux-gnu",
}

HEAD = '''class AtlasCodeNightly < Formula
  desc "Draws what a program is, beneath its language: the nightly build"
  homepage "https://raoulg.github.io/atlas/"
  version "{version}"
  license "CC-BY-NC-SA-4.0"

  # A nightly of `next`, built when `next` has moved and its tests are green: one pre-release per build.
  # It installs `atlas-nightly`, beside the release's `atlas`.
'''

TAIL = '''
  def install
    bin.install "atlas" => "atlas-nightly"
  end

  test do
    assert_match version.to_s.split("-").first, shell_output("#{bin}/atlas-nightly --version")
  end
end
'''


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def formula(dist: Path, base: str, stamp: str) -> str:
    out = HEAD.format(version=f"{base}-nightly.{stamp}")
    for os_block in ("on_macos", "on_linux"):
        out += f"\n  {os_block} do\n"
        for cpu in ("on_arm", "on_intel"):
            target = TARGETS[(os_block, cpu)]
            out += f'    {cpu} do\n      url "{URL.format(stamp=stamp, target=target)}"\n'
            out += f'      sha256 "{sha256(dist / f"atlas-{target}.tar.gz")}"\n    end\n'
        out += "  end\n"
    return out + TAIL


if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    sys.stdout.write(formula(Path(sys.argv[1]), sys.argv[2], sys.argv[3]))
