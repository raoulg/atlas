# Install

The README has the one-line installs. This page is for everything after: which channel you are on,
how updates reach you, and how your coding agent gets Atlas.

## The command

The command is `atlas-code`: other tools (ariga's database toolkit, MongoDB's Atlas CLI) install an `atlas`, and
two programs cannot share one command. For the short name, add an alias to your shell's configuration
(`alias atlas=atlas-code` in `~/.zshrc` or `~/.bashrc`, `Set-Alias atlas atlas-code` in your PowerShell profile).
An `atlas` that Atlas before 0.70 installed goes when you update: `install.sh`, `install.ps1` and `atlas update`
remove it, and `brew upgrade` takes it away with the old version.

## Stable and nightly

`main` is the stable line: only fixes, each a tagged release. `next` is where new work lands; a
nightly build is made of it when its tests are green. The nightly installs as a command of its own,
`atlas-code-nightly`, beside the release's `atlas-code`, so the two can be installed together and the release is
at hand when a nightly breaks. The scripts take the channel from `ATLAS_CHANNEL`:

```bash
ATLAS_CHANNEL=nightly sh -c "$(curl -fsSL https://raw.githubusercontent.com/raoulg/atlas/next/install.sh)"
```

```powershell
$env:ATLAS_CHANNEL = 'nightly'; irm https://raw.githubusercontent.com/raoulg/atlas/next/install.ps1 | iex
```

Homebrew has no channel variable; the nightly is a formula of its own, built from the binaries of the
nightly release. On the command line, and in a `Brewfile`:

```bash
brew install raoulg/tap/atlas-code raoulg/tap/atlas-code-nightly
brew upgrade atlas-code-nightly
```

```ruby
brew "raoulg/tap/atlas-code"              # the release: `atlas-code`
brew "raoulg/tap/atlas-code-nightly"      # the nightly, beside it: `atlas-code-nightly`
```

Homebrew loads a formula from a tap it does not ship only once it is trusted; naming it in full, as
here, trusts that formula alone. `brew install --HEAD raoulg/tap/atlas-code` builds `next` from source
instead (it needs Rust), and installs it as `atlas-code`, as `cargo install` does.

`atlas-code --version` says `atlas-code 0.71.0`, `atlas-code-nightly --version` says `atlas-code 0.71.0-nightly (b0ead14)`.

## Atlas Studio

Atlas Studio is the desktop app around the same engine, with the matching `atlas-code` inside it: it
needs no installed CLI. Every release and every nightly has a disk image per Mac,
`atlas-studio-aarch64-apple-darwin.dmg` (Apple silicon) and `atlas-studio-x86_64-apple-darwin.dmg`
(Intel), and a setup program for Windows, `atlas-studio-x86_64-pc-windows-msvc-setup.exe`. On a Mac
Homebrew puts the app in `/Applications`:

```bash
brew install --cask raoulg/tap/atlas-studio
```

`raoulg/tap/atlas-studio-nightly` is the nightly: an app of its own, Atlas Studio Nightly, with its own
settings and list of workspaces, so the release and the nightly install side by side and the release
is at hand when a nightly breaks. Open a repository in one of them at a time: both would run
`atlas-code serve` on what it keeps in `.atlas/`. In a `Brewfile` it is `cask "raoulg/tap/atlas-studio"`
(or `-nightly`, or both). Studio is signed ad hoc, not with an Apple Developer ID. Homebrew
marks what it downloads as from the internet, as a browser does, so that macOS checks the app, and
macOS blocks its first start: open Studio once, then allow it in System Settings › Privacy & Security
› Open Anyway. Studio installs its own updates (Atlas Studio › Check for Updates…), which open without
asking again. Homebrew leaves an app that updates itself to do so: `brew upgrade` passes Studio by, and
`brew upgrade --greedy` installs a new copy, which macOS asks about once more.

The script puts the app in `~/Applications` instead:

```bash
sh -c "$(curl -fsSL https://raw.githubusercontent.com/raoulg/atlas/main/install-studio.sh)"
```

`ATLAS_CHANNEL=nightly` installs the nightly, as `Atlas Studio Nightly.app` beside the release, and
`ATLAS_STUDIO_DIR` another folder. Run it again to update. The script downloads with `curl`, which leaves no mark of the internet on the app, so it opens
as it is. A disk image downloaded in a browser does carry that mark: drag the app to Applications,
open it once, and then allow it in System Settings › Privacy & Security › Open Anyway.

On Windows the script installs Studio for you alone, without administrator rights, in
`%LOCALAPPDATA%\Atlas Studio`, with an entry in the Start menu:

```powershell
irm https://raw.githubusercontent.com/raoulg/atlas/main/install-studio.ps1 | iex
```

`$env:ATLAS_CHANNEL = 'nightly'` installs the nightly, as Atlas Studio Nightly in
`%LOCALAPPDATA%\Atlas Studio Nightly` beside the release; run it again to update. The setup program is
not signed with a paid identity either. Downloaded by the script it carries no mark of the internet
and runs as it is; downloaded in a browser, SmartScreen asks first: choose More info, then Run anyway.
Windows › Settings › Apps removes Studio again.

## Updates

Once a day, `atlas-code serve`, `view`, `check`, `changes`, `history` and `revelation` tell you on stderr when a
newer build of your channel is out, with the command that upgrades the way you installed it
(`brew upgrade atlas-code`, the install script, or `cargo install`). It asks GitHub once a day through
`curl`, in the background, and stays silent when stderr is not a terminal, in CI, or with
`ATLAS_NO_UPDATE_CHECK=1`.

## Your coding agent

Your agent gets Atlas through `atlas-code mcp`. Register it once, for every project:

```bash
claude mcp add --scope user atlas -- atlas-code mcp
codex mcp add atlas -- "$(command -v atlas-code)" mcp
```

An agent has one server called `atlas`: give it the nightly with `atlas-code-nightly mcp` in place of
`atlas-code mcp`. Studio's Settings › Coding agents registers the CLI of its own channel, else its own
`atlas-code`, so that the agent's `atlas-code mcp` is of the channel of the `atlas-code serve` it shares the page with.

An agent registered with `atlas mcp` (Atlas before 0.70) runs a command that is no longer installed: register it
again, as above (`claude mcp remove atlas` first; Codex: `codex mcp remove atlas`).

Claude Code's `--scope user` makes it available in every project. Codex keeps its servers in one
configuration for all projects and starts them without your shell's `PATH`, so it is given the
binary's full path, which `command -v atlas-code` prints. From then on `atlas-code serve` in any repository
is all it takes: the agent's `atlas-code mcp` works on the repository the agent runs in, and shares the
page with you.

## Nothing to install on your machine

On a managed laptop, say: a GitHub workflow draws the page on GitHub's runners and publishes it, to
open in the browser. See [the picture in your README, always current](ci.md).
