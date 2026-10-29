#!/usr/bin/env python3
"""The contract between what the tap jobs write and the release they name, as Homebrew reads it: the casks of
`tap_cask.py` and the formulas of `tap_formula.py`.

On every platform and architecture Homebrew knows, each resolves to a file of that build's release (the tag
`v<version>`, or the pre-release `nightly-<stamp>`), for that architecture, with that file's checksum;
Homebrew's own `brew style` and `brew audit` accept it; and the release and the nightly install side by
side: two apps with their own data, and two commands that exclude no formula. Homebrew reads a cask or
formula only from a tap, so the test makes one and removes it again. It needs `brew`, and skips without it
(CI runs it on macOS).

    python3 tools/test_tap.py
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
from tap_formula import ARCHIVE, TARGETS, formula  # noqa: E402

RELEASES = "https://github.com/raoulg/atlas/releases/download/"
BREW = shutil.which("brew")
# The test's own configuration folder, where `brew trust` keeps its trust of the test's taps.
ENV = {
    **os.environ,
    "HOMEBREW_NO_AUTO_UPDATE": "1",
    "HOMEBREW_NO_INSTALL_FROM_API": "1",
    "HOMEBREW_NO_ENV_HINTS": "1",
    "XDG_CONFIG_HOME": tempfile.mkdtemp(),
}


def brew(*args: str) -> str:
    done = subprocess.run([BREW, *args], env=ENV, capture_output=True, text=True)
    if done.returncode != 0:
        raise AssertionError(f"brew {' '.join(args)}:\n{done.stdout}{done.stderr}")
    return done.stdout


def tap_new(name: str) -> None:
    """A tap of the test's own that Homebrew loads (a Homebrew from before tap trust loads it anyway)."""
    brew("tap-new", "--no-git", name)
    subprocess.run([BREW, "trust", "--tap", name], env=ENV, capture_output=True)


@unittest.skipUnless(BREW, "needs Homebrew")
class CaskNamesItsBuild(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.dist = Path(tempfile.mkdtemp())
        for arch in ARCHES.values():
            (cls.dist / DMG.format(arch=arch)).write_bytes(f"the disk image for {arch}".encode())
        cls.tap = f"atlas-test/cask-{os.getpid()}"
        tap_new(cls.tap)
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

    def test_the_two_channels_install_side_by_side(self):
        release = self.read("atlas-studio", cask(self.dist, "1.2.3"))
        nightly = self.read("atlas-studio-nightly", cask(self.dist, "1.3.0", "202610261819"))
        self.assertNotIn("atlas-studio-nightly", (release["conflicts_with"] or {}).get("cask", []))
        self.assertNotIn("atlas-studio", (nightly["conflicts_with"] or {}).get("cask", []))

        def apps(read):
            return {app for artifact in read["artifacts"] for app in artifact.get("app", [])}

        def data(read):
            return {path for artifact in read["artifacts"] for zap in artifact.get("zap", []) for path in zap["trash"]}

        self.assertTrue(apps(release) and apps(nightly))
        self.assertFalse(apps(release) & apps(nightly), "both install one app")
        self.assertTrue(data(release) and data(nightly))
        self.assertFalse(data(release) & data(nightly), "zapping one removes the other's data")


@unittest.skipUnless(BREW, "needs Homebrew")
class FormulaNamesItsBuild(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.dist = Path(tempfile.mkdtemp())
        for target in TARGETS.values():
            (cls.dist / ARCHIVE.format(target=target)).write_bytes(f"the archive for {target}".encode())
        cls.tap = f"atlas-test/formula-{os.getpid()}"
        tap_new(cls.tap)
        cls.formulae = Path(brew("--repo", cls.tap).strip()) / "Formula"
        cls.formulae.mkdir(exist_ok=True)
        (cls.formulae / "atlas-code.rb").write_text(formula(cls.dist, "1.3.0"))
        (cls.formulae / "atlas-code-nightly.rb").write_text(formula(cls.dist, "1.3.0", "202610261819"))
        cls.release, cls.nightly = f"{cls.tap}/atlas-code", f"{cls.tap}/atlas-code-nightly"

    @classmethod
    def tearDownClass(cls):
        brew("untap", "--force", cls.tap)
        shutil.rmtree(cls.dist)

    def resolves_to(self, name: str, version: str, tag: str):
        """Every variation's URL is an archive of `tag` whose file carries the checksum named beside it."""
        read = json.loads(brew("info", "--formula", "--json=v2", "--variations", name))["formulae"][0]
        self.assertEqual(read["versions"]["stable"], version)
        by_file = {ARCHIVE.format(target=target): sha256(self.dist / ARCHIVE.format(target=target)) for target in TARGETS.values()}
        seen = set()
        for where, variation in [("this machine", read), *(read.get("variations") or {}).items()]:
            stable = (variation.get("urls") or read["urls"])["stable"]
            url, sha = stable["url"], stable["checksum"]
            self.assertTrue(url.startswith(f"{RELEASES}{tag}/"), f"{where}: {url}")
            file = url.rsplit("/", 1)[1]
            self.assertEqual(sha, by_file.get(file), f"{where}: {file} with another file's checksum")
            seen.add(file)
        self.assertEqual(seen, set(by_file), "an archive no platform installs")

    def test_a_release_names_its_tag(self):
        self.resolves_to(self.release, "1.3.0", "v1.3.0")

    def test_a_nightly_names_its_prerelease(self):
        self.resolves_to(self.nightly, "1.3.0-nightly.202610261819", "nightly-202610261819")

    def test_the_two_channels_install_side_by_side(self):
        for name in (self.release, self.nightly):
            read = json.loads(brew("info", "--formula", "--json=v2", name))["formulae"][0]
            self.assertEqual(read["conflicts_with"], [], f"{name} excludes another formula")
        source = {name: (self.formulae / f"{name.rsplit('/', 1)[1]}.rb").read_text() for name in (self.release, self.nightly)}
        self.assertIn('bin.install "atlas-code"\n', source[self.release])
        self.assertIn('bin.install "atlas-code" => "atlas-code-nightly"', source[self.nightly], "both install one command")

    def test_homebrew_accepts_them(self):
        for name in (self.release, self.nightly):
            brew("style", name)
            brew("audit", "--formula", "--strict", name)


if __name__ == "__main__":
    unittest.main()
