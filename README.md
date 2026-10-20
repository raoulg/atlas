<p align="center">
  <img src="docs/logo.svg" width="112" alt="Atlas logo: an A found in the cellular automaton Rule 30, five of its cells joined as a route">
</p>

<h1 align="center">Atlas</h1>

<p align="center">
  <b>What a program is, beneath the language it is written in.</b>
</p>

<p align="center">
  <a href="https://raoulg.github.io/atlas/"><b>Take the tour</b></a> ·
  <a href="#install">Install</a> ·
  <a href="docs/llms.txt">For agents</a>
</p>

<p align="center">
  <a href="https://claude.com/claude-code"><img src="https://img.shields.io/badge/built%20with-Claude-D97757?logo=claude&logoColor=white" alt="Built with Claude"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-CC%20BY--NC--SA%204.0-lightgrey" alt="CC BY-NC-SA 4.0"></a>
  <img src="https://img.shields.io/badge/status-alpha-C43A14" alt="Alpha">
  <img src="https://img.shields.io/badge/reads-Python%20·%20Rust%20·%20TypeScript-555" alt="Reads Python, Rust and TypeScript">
</p>

Two programs can show you the same screen. One is sloppy code with lots of shortcuts and
inconsistent structure. The other is an elegant orchestration of logic. Atlas draws what each one
is, so that you and your coding agent can look at the same picture, and you can point at it.

## The same figure from two very different programs

Fetch sunspot counts from the web, keep the CSV, clean it, smooth it, draw a figure. Written twice
in Python: once as a sloppy script, once typed and elegant. Both make this figure.

<p align="center">
  <img src="docs/img/showcase/sunspots.png" width="60%" alt="SILSO's monthly mean sunspot number since 1749, a thin grey line, with its 13-month mean in blue over it: the eleven-year solar cycle">
</p>

<h3 align="center">A sloppy script</h3>
<p align="center">
  <a href="https://raoulg.github.io/atlas/showcase/python-sloppy.html"><img src="docs/img/showcase/python-sloppy.png" width="100%" alt="Atlas's picture of the sloppy script: every step reads and changes one global, Global(sunspots.df), and two of them pick their own files"></a>
</p>

<h3 align="center">Typed and elegant</h3>
<p align="center">
  <a href="https://raoulg.github.io/atlas/showcase/python-clean.html"><img src="docs/img/showcase/python-clean.png" width="100%" alt="Atlas's picture of the typed pipeline: download runs only when no raw CSV is kept, the CSV stands between download and load, a DataFrame passes from load to smooth to Trend.save, and the command names every file"></a>
</p>

Both reach the same places: the web, the raw CSV, the figure. Inside they part. In the sloppy
script every step reads and changes one global, `Global(sunspots.df)`, and nothing is handed from
step to step. In the typed one the CSV stands between `download` and `load`, a `DataFrame` goes
from step to step, and the command names every file. A red dot is a question Atlas asks the
author. Click a picture to open it in Atlas.

## What you do with it

[**Take the tour**](https://raoulg.github.io/atlas/): every step below, drawn from a real run.

- [**Port it, and see whether the design survived.**](https://raoulg.github.io/atlas/#languages) The same pipeline in Python, Rust and TypeScript, in the same picture.
- [**See what a change did, without reading the diff.**](https://raoulg.github.io/atlas/#changed) Two commits as one picture.
- [**Say exactly what you mean: point at it.**](https://raoulg.github.io/atlas/#point) Notes on a box, and the agent's answer on it.
- [**Design the next feature before you write it.**](https://raoulg.github.io/atlas/#design) A vision, then a prophecy, then the code.

## Install

**macOS and Linux:**

```bash
curl -fsSL https://raw.githubusercontent.com/raoulg/atlas/main/install.sh | sh
```

**Windows**, in PowerShell:

```powershell
irm https://raw.githubusercontent.com/raoulg/atlas/main/install.ps1 | iex
```

**Homebrew** (the formula is `atlas-code`: homebrew-core has an `atlas` of its own):

```bash
brew install raoulg/tap/atlas-code
```

**From source**, with a [Rust toolchain](https://rustup.rs):

```bash
cargo install --locked --git https://github.com/raoulg/atlas atlas
```

### Stable and nightly

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

**Nothing to install on your machine**, a managed laptop for instance: a GitHub workflow draws the
page on GitHub's runners and publishes it, to open in the browser. See
[the picture in your README, always current](docs/reference/ci.md).

`atlas --version` says which one you have: `0.66.2`, or `0.67.0-nightly (b0ead14)`.

Once a day, `atlas serve`, `view`, `check`, `changes`, `history` and `revelation` tell you on stderr when a
newer build of your channel is out, with the command that upgrades the way you installed it
(`brew upgrade atlas-code`, the install script, or `cargo install`). It asks GitHub once a day through
`curl`, in the background, and stays silent when stderr is not a terminal, in CI, or with
`ATLAS_NO_UPDATE_CHECK=1`.

## Use

```bash
atlas serve      # the picture of the repository you are in, to work in with your agent
atlas view       # the same picture as one HTML file, to keep or send
atlas check      # where the code and eleven design principles disagree
atlas revelation # the sketch code kept in the tree, and what is left to build
atlas changes    # what your edit did to the routes
atlas mcp        # the picture for your coding agent
```

**Your agent** gets Atlas through `atlas mcp`. Register it once, for every project:

```bash
claude mcp add --scope user atlas -- atlas mcp
codex mcp add atlas -- "$(command -v atlas)" mcp
```

Claude Code's `--scope user` makes it available in every project. Codex keeps its servers in one
configuration for all projects and starts them without your shell's `PATH`, so it is given the
binary's full path, which `command -v atlas` prints. From then on `atlas serve` in any repository
is all it takes: the agent's `atlas mcp` works on the repository the agent runs in, and shares the
page with you.

**Your README** can show the picture of the code as it is: a GitHub workflow draws it anew on every
push, or a commit hook before every commit. The workflow can also check every pull request and draw
what it changed. [How to set it up](docs/reference/ci.md).

## More

- [**The tour**](https://raoulg.github.io/atlas/): the same pictures on one page, to click through.
- [**The picture in your README, always current**](docs/reference/ci.md): a GitHub workflow or a commit hook.
- [**The guide**](docs/reference/guide.md), [**the principles**](docs/reference/principles.md), [**the mathematics**](docs/reference/mathematics.md) and [**the design**](docs/reference/design.md).
- [**For agents**](docs/llms.txt): what Atlas is for and how an agent should use it, then the design,
  the principles and the guide, written to be read in one go.

Alpha: what Atlas reads and draws changes from one minor version to the next. Built by Raoul Grouls
with [Claude Code](https://claude.com/claude-code). [CC BY-NC-SA 4.0](LICENSE).
