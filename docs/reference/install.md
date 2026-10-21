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
