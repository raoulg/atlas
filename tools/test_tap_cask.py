#!/usr/bin/env python3
"""The contract between the casks `tap_cask.py` writes and the release they name, as Homebrew reads them.

For a release and for a nightly: on every macOS and architecture Homebrew knows, the cask resolves to
a disk image of that build's release (the tag `v<version>`, or the pre-release `nightly-<stamp>`), for
that architecture, with that file's checksum; and Homebrew's own `brew style` and `brew audit` accept
the cask. Homebrew reads a cask only from a tap, so the test makes one and removes it again. It needs
`brew`, and skips without it (CI runs it on macOS).

    python3 tools/test_tap_cask.py
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from tap_cask import ARCHES, DMG, cask, sha256  # noqa: E402

RELEASES = "https://github.com/raoulg/atlas/releases/download/"
BREW = shutil.which("brew")
ENV = {**os.environ, "HOMEBREW_NO_AUTO_UPDATE": "1", "HOMEBREW_NO_INSTALL_FROM_API": "1", "HOMEBREW_NO_ENV_HINTS": "1"}


def brew(*args: str) -> str:
    done = subprocess.run([BREW, *args], env=ENV, capture_output=True, text=True)
    if done.returncode != 0:
        raise AssertionError(f"brew {' '.join(args)}:\n{done.stdout}{done.stderr}")
    return done.stdout


@unittest.skipUnless(BREW, "needs Homebrew")
class CaskNamesItsBuild(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.dist = Path(tempfile.mkdtemp())
        for arch in ARCHES.values():
            (cls.dist / DMG.format(arch=arch)).write_bytes(f"the disk image for {arch}".encode())
        cls.tap = f"atlas-test/cask-{os.getpid()}"
        brew("tap-new", "--no-git", cls.tap)
        cls.casks = Path(brew("--repo", cls.tap).strip()) / "Casks"
        cls.casks.mkdir(exist_ok=True)

    @classmethod
    def tearDownClass(cls):
        brew("untap", "--force", cls.tap)
        shutil.rmtree(cls.dist)

    def read(self, token: str, text: str) -> dict:
        (self.casks / f"{token}.rb").write_text(text)
        return json.loads(brew("info", "--cask", "--json=v2", "--variations", f"{self.tap}/{token}"))["casks"][0]

    def resolves_to(self, read: dict, tag: str):
        """Every variation's URL is an asset of `tag` whose file carries the checksum named beside it."""
        by_file = {DMG.format(arch=arch): sha256(self.dist / DMG.format(arch=arch)) for arch in ARCHES.values()}
        seen = set()
        for name, variation in [("this Mac", read), *read["variations"].items()]:
            if name.endswith("_linux"):  # the cask is for macOS alone (`depends_on :macos`)
                continue
            url, sha = variation.get("url", read["url"]), variation.get("sha256", read["sha256"])
            self.assertTrue(url.startswith(f"{RELEASES}{tag}/"), f"{name}: {url} is not of {tag}")
            file = url.rsplit("/", 1)[1]
            self.assertIn(file, by_file, f"{name}: {url} names no disk image of the build")
            self.assertEqual(sha, by_file[file], f"{name}: {file} with another file's checksum")
            arch = ARCHES["arm"] if name.startswith("arm64_") else ARCHES["intel"]
            if name != "this Mac":
                self.assertEqual(file, DMG.format(arch=arch), f"{name}: the disk image of another architecture")
            seen.add(file)
        self.assertEqual(seen, set(by_file), "a disk image no Mac installs")

    def accepted(self, token: str):
        brew("style", f"{self.tap}/{token}")
        brew("audit", "--cask", "--strict", f"{self.tap}/{token}")

    def test_a_release_names_its_tag(self):
        read = self.read("atlas-studio", cask(self.dist, "1.2.3"))
        self.assertEqual(read["version"], "1.2.3")
        self.resolves_to(read, "v1.2.3")
        self.accepted("atlas-studio")

    def test_a_nightly_names_its_prerelease(self):
        read = self.read("atlas-studio-nightly", cask(self.dist, "1.3.0", "202610261819"))
        self.assertEqual(read["version"], "1.3.0-nightly.202610261819")
        self.resolves_to(read, "nightly-202610261819")
        self.accepted("atlas-studio-nightly")

    def test_the_two_channels_exclude_each_other(self):
        # both install `Atlas Studio.app`
        release = self.read("atlas-studio", cask(self.dist, "1.2.3"))
        nightly = self.read("atlas-studio-nightly", cask(self.dist, "1.3.0", "202610261819"))
        self.assertIn("atlas-studio-nightly", release["conflicts_with"]["cask"])
        self.assertIn("atlas-studio", nightly["conflicts_with"]["cask"])


if __name__ == "__main__":
    unittest.main()
