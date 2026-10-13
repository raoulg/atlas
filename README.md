<p align="center">
  <img src="docs/logo.svg" width="112" alt="Atlas logo: an A found in the cellular automaton Rule 30, five of its cells joined as a route">
</p>

<h1 align="center">Atlas</h1>

<p align="center">
  <b>What a program is, beneath the language it is written in.</b>
</p>

<p align="center">
  <a href="https://raoulg.github.io/atlas/"><b>See it</b></a> ·
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
inconsistent structure. The other is an elegant orchestration of logic. Atlas shows which is which.

<h3 align="center">A sloppy script</h3>
<p align="center">
  <a href="https://raoulg.github.io/atlas/showcase/python-sloppy.html"><img src="docs/img/showcase/python-sloppy.png" width="100%" alt="Atlas's picture of the sloppy script: every step reads and changes one global, Global(sunspots.df), and two of them pick their own files"></a>
</p>

<h3 align="center">Typed and elegant</h3>
<p align="center">
  <a href="https://raoulg.github.io/atlas/showcase/python-clean.html"><img src="docs/img/showcase/python-clean.png" width="100%" alt="Atlas's picture of the typed pipeline: a DataFrame passes from load to smooth to Trend.save, and the command names every file"></a>
</p>

<p align="center"><sub>
The same pipeline: fetch sunspot counts from the web, keep the CSV, clean it, smooth it, draw a
figure. The same places at the edges, the same figure out. A red dot is a question Atlas asks the
author; here: why do <code>get_data</code> and <code>plot</code> pick their own files, and what do
the four functions give back? Click a picture to open it.
</sub></p>

## The mathematics behind it

Atlas reads a program the way category theory reads mathematics. Types are objects, functions are
arrows between them, and a program is what you get by composing arrows: call order is
composition. A function that writes a file or asks a database does more than turn its input into
its output, so Atlas counts that as part of its type, as an effect system does. Zooming out folds a
family of types into one box; keeping to one route picks out the part of the picture where it runs.
What Atlas checks comes from type theory: a sum type is unwrapped where it arises, an override
keeps the promise of its base, what a lens puts it gets back.

None of this belongs to Python, TypeScript or Rust. That makes Atlas a proof machine whose proofs
are pictures: every box and arrow is derived from the code, nothing is guessed, so a picture that
looks wrong is a fact about the code. And the same program in two languages gives the same picture,
so a port can be laid over its original. Where the pictures differ, the languages do.

## Install

**macOS and Linux:**

```bash
curl -fsSL https://raw.githubusercontent.com/raoulg/atlas/main/install.sh | sh
```

**Windows**, in PowerShell:

```powershell
irm https://raw.githubusercontent.com/raoulg/atlas/main/install.ps1 | iex
```

**From source**, with a [Rust toolchain](https://rustup.rs):

```bash
cargo install --locked --git https://github.com/raoulg/atlas atlas
```

## Use

```bash
atlas view      # draw the repository you are in
atlas check     # where the code and ten design principles disagree
atlas changes   # what your edit did to the routes
atlas mcp       # the same picture for your coding agent
```

## More

- [**See it**](https://raoulg.github.io/atlas/): the examples, side by side, to click through, and the
  same program ported to TypeScript and Rust.
- [**For agents**](docs/llms.txt): what Atlas is for and how an agent should use it, then the design,
  the principles and the guide, written to be read in one go.

Alpha: what Atlas reads and draws changes from one minor version to the next. Built by Raoul Grouls
with [Claude Code](https://claude.com/claude-code). [CC BY-NC-SA 4.0](LICENSE).
