# Install

The README has the one-line installs. This page is for everything after: which channel you are on,
how updates reach you, and how your coding agent gets Atlas.

## Stable and nightly

`main` is the stable line: only fixes, each a tagged release. `next` is where new work lands; a
nightly build is made of it when its tests are green. The scripts take the channel from
`ATLAS_CHANNEL`:

```bash
ATLAS_CHANNEL=nightly sh -c "$(curl -fsSL https://raw.githubusercontent.com/raoulg/atlas/next/install.sh)"
```

```powershell
$env:ATLAS_CHANNEL = 'nightly'; irm https://raw.githubusercontent.com/raoulg/atlas/next/install.ps1 | iex
```

Homebrew has no channel variable; the nightly is a formula of its own, built from the binaries of the
nightly release, and it replaces the stable one (both install `atlas`). On the command line, and in a
`Brewfile`:

```bash
brew install raoulg/tap/atlas-code-nightly
brew upgrade atlas-code-nightly
```

```ruby
tap "raoulg/tap"
brew "atlas-code-nightly"                 # the nightly; `brew "atlas-code"` is the stable release
```

`brew install --HEAD raoulg/tap/atlas-code` builds `next` from source instead (it needs Rust).

`atlas --version` says which one you have: `0.66.2`, or `0.67.0-nightly (b0ead14)`.

## Atlas Studio

Atlas Studio is the desktop app around the same engine, with the matching `atlas` inside it: it
needs no installed CLI. Every release and every nightly has a disk image per Mac,
`atlas-studio-aarch64-apple-darwin.dmg` (Apple silicon) and `atlas-studio-x86_64-apple-darwin.dmg`
(Intel), and a setup program for Windows, `atlas-studio-x86_64-pc-windows-msvc-setup.exe`. On a Mac
the script puts the app in `~/Applications`:

```bash
sh -c "$(curl -fsSL https://raw.githubusercontent.com/raoulg/atlas/main/install-studio.sh)"
```

`ATLAS_CHANNEL=nightly` installs the nightly, `ATLAS_STUDIO_DIR` another folder. Run it again to
update. Studio is signed ad hoc, not with an Apple Developer ID. The script downloads with `curl`,
which leaves no mark of the internet on the app, so it opens as it is. A disk image downloaded in a
browser does carry that mark: drag the app to Applications, open it once, and then allow it in System
Settings › Privacy & Security › Open Anyway (or Control-click the app and choose Open).

On Windows the script installs Studio for you alone, without administrator rights, in
`%LOCALAPPDATA%\Atlas Studio`, with an entry in the Start menu:

```powershell
irm https://raw.githubusercontent.com/raoulg/atlas/main/install-studio.ps1 | iex
```

`$env:ATLAS_CHANNEL = 'nightly'` installs the nightly; run it again to update. The setup program is
not signed with a paid identity either. Downloaded by the script it carries no mark of the internet
and runs as it is; downloaded in a browser, SmartScreen asks first: choose More info, then Run anyway.
Windows › Settings › Apps removes Studio again.

## Updates

Once a day, `atlas serve`, `view`, `check`, `changes`, `history` and `revelation` tell you on stderr when a
newer build of your channel is out, with the command that upgrades the way you installed it
(`brew upgrade atlas-code`, the install script, or `cargo install`). It asks GitHub once a day through
`curl`, in the background, and stays silent when stderr is not a terminal, in CI, or with
`ATLAS_NO_UPDATE_CHECK=1`.

## Your coding agent

Your agent gets Atlas through `atlas mcp`. Register it once, for every project:

```bash
claude mcp add --scope user atlas -- atlas mcp
codex mcp add atlas -- "$(command -v atlas)" mcp
```

Claude Code's `--scope user` makes it available in every project. Codex keeps its servers in one
configuration for all projects and starts them without your shell's `PATH`, so it is given the
binary's full path, which `command -v atlas` prints. From then on `atlas serve` in any repository
is all it takes: the agent's `atlas mcp` works on the repository the agent runs in, and shares the
page with you.

## Nothing to install on your machine

On a managed laptop, say: a GitHub workflow draws the page on GitHub's runners and publishes it, to
open in the browser. See [the picture in your README, always current](ci.md).
