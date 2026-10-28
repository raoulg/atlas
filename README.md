<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/img/band-dark.svg">
    <img src="docs/img/band-light.svg" width="100%" alt="Atlas: the cellular automaton Rule 30 as a band, its logo standing in it, an A that Rule 30 holds, five of its cells joined as a route">
  </picture>
</p>

<h1 align="center">Atlas</h1>

<p align="center">
  <a href="https://raoulg.github.io/atlas/"><b>Take the tour</b></a> ·
  <a href="#install">Install</a> ·
  <a href="https://raoulg.github.io/atlas/releases/">What's new</a> ·
  <a href="docs/llms.txt">For agents</a>
</p>

<p align="center">
  <a href="https://claude.com/claude-code"><img src="https://img.shields.io/badge/built%20with-Claude-D97757?logo=claude&logoColor=white" alt="Built with Claude"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-CC%20BY--NC--SA%204.0-lightgrey" alt="CC BY-NC-SA 4.0"></a>
  <a href="https://raoulg.github.io/atlas/"><img src="https://raoulg.github.io/atlas/badge.svg" alt="atlas: drawn on push"></a>
  <a href="https://raoulg.github.io/atlas/atlas.html"><img src="https://raoulg.github.io/atlas/questions.svg" alt="atlas: questions it asks of its own code"></a>
  <img src="https://img.shields.io/badge/status-alpha-C43A14" alt="Alpha">
  <img src="https://img.shields.io/badge/reads-Python%20·%20Rust%20·%20TypeScript-555" alt="Reads Python, Rust and TypeScript">
</p>

Atlas draws your code as a small piece of mathematics: objects, functions, and the topology of
how they are connected. You and your coding agent look at the same picture, which makes it easier
to collaborate.

## The same figure from two very different programs

Smooth the monthly sunspot counts since 1749 and draw them. Two programs make exactly this
figure, one a sloppy script and one typed and elegant, and the output cannot tell them apart.
Atlas can.

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

Click a picture to open it live in Atlas. [The tour](https://raoulg.github.io/atlas/#example) says
what to see in them.

## What you do with it

[**Take the tour**](https://raoulg.github.io/atlas/): every step below, drawn from a real run.

- [**Port it, and see whether the design survived.**](https://raoulg.github.io/atlas/#languages) The same pipeline in Python, Rust and TypeScript, in the same picture.
- [**Say exactly what you mean: point at it.**](https://raoulg.github.io/atlas/#point) Notes on a box, and the agent's answer on it.
- [**Design the next feature before you write it.**](https://raoulg.github.io/atlas/#design) A vision, then a prophecy, then the code.
- [**See what a change did, without reading the diff.**](https://raoulg.github.io/atlas/#changed) Two commits as one picture.
- [**Let your README draw itself.**](https://raoulg.github.io/atlas/#readme) Every push redraws the picture of your code, on GitHub ([how](docs/reference/ci.md)).

## Atlas, drawn by Atlas

Every push to `main` draws Atlas's own `analyze` route, the part that reads a repository, and the
workflow behind it is [one you can copy](docs/reference/ci.md). Click the picture to open it live.

<p align="center">
  <a href="https://raoulg.github.io/atlas/atlas.html">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://raoulg.github.io/atlas/atlas-dark.png">
      <img src="https://raoulg.github.io/atlas/atlas.png" width="100%" alt="The picture Atlas draws of its own analyze route: a repository is read, then analyzed into a graph">
    </picture>
  </a>
</p>

## Install

Atlas comes in two forms, around the same engine:

- **The command line**, `atlas`, for macOS, Linux and Windows. `atlas view .` opens the picture of a
  repository in your browser, and `atlas serve` keeps that page open while you and your agent work.
- **Atlas Studio**, a desktop app for macOS (Apple silicon and Intel) and Windows: the picture, its
  history and a terminal in one window, one workspace per repository. It carries its own `atlas`, so
  it needs no installed command line.

What changed in each release, with pictures: [the release notes](https://raoulg.github.io/atlas/releases/).

### The command line

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

**Nothing to install on your machine**, a managed laptop for instance: a GitHub workflow draws the
page on GitHub's runners and publishes it, to open in the browser. See
[the picture in your README, always current](docs/reference/ci.md).

### Atlas Studio

**macOS**, with Homebrew, into `/Applications`:

```bash
brew install --cask raoulg/tap/atlas-studio
```

or with the script, into `~/Applications`:

```bash
sh -c "$(curl -fsSL https://raw.githubusercontent.com/raoulg/atlas/main/install-studio.sh)"
```

**Windows**, in PowerShell, for you alone and without administrator rights:

```powershell
irm https://raw.githubusercontent.com/raoulg/atlas/main/install-studio.ps1 | iex
```

Every [release](https://github.com/raoulg/atlas/releases/latest) also has the disk images
(`atlas-studio-aarch64-apple-darwin.dmg` for Apple silicon, `atlas-studio-x86_64-apple-darwin.dmg` for
Intel) and the Windows setup program. Studio is not signed with a paid identity, so macOS and Windows
ask once before its first start: [how to allow it](docs/reference/install.md#atlas-studio).

### After installing

The nightly channel, how Atlas and Studio tell you about updates, and registering Atlas with your
coding agent: [install](docs/reference/install.md).

Alpha: what Atlas reads and draws changes from one minor version to the next. Built by Raoul Grouls
with [Claude Code](https://claude.com/claude-code). [CC BY-NC-SA 4.0](LICENSE).
