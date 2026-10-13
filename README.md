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

Atlas draws mathematics. Beneath it lies a small piece of
[category theory](https://pttrn.io/#content/category-theory), and every construct in it has one
shape: data at rest is a box of its kind, a function is an arrow, a route runs from a way in to the
places it reaches. Every box and arrow is derived from the code, nothing is guessed, and every
program is drawn by the same rules. So you need none of the mathematics to read the picture:
consistent code draws as a consistent picture, and where the picture looks off, so is the code.

The rules don't belong to Python, TypeScript or Rust. The same program in two languages draws the
same picture, so a port can be laid over its original. Where the pictures differ, the languages do.

<details>
<summary><b>What is inside</b>, for the mathematically inclined</summary>
<br>

- **A category of types.** Types are objects, functions are arrows, call order is composition. A
  route is a composite from an entry point to a place.
- **Effects in the type.** A function's domain is its parameters plus what it reads; its codomain
  is what it returns plus what it writes, sends or runs. Files, tables and URLs are objects like
  any other, as in an effect system.
- **Zoom is a quotient, a filter is a fiber.** Zooming out folds a family of types (by base
  class, by composition, by module) into one box; keeping to one route is the fiber over it. The
  two don't commute, so the order of a view is meaning.
- **Sum types are coproducts**, taken apart where they arise.
- **Lawful lenses.** A class that writes a place and reads it back is a lens on that place. Atlas
  asks the put-get law at the level of types: what goes in as a `Graph` comes back as a `Graph`.
- **Subtypes keep their promise.** An override does what its base says (Liskov).
- **A step and its reach are different quantities.** A function's own arrow and the composite it
  starts are never merged.

The structure is laid out in [the design](docs/reference/design.md); what Atlas checks against it
is in [the ten principles](docs/reference/principles.md).
</details>

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
