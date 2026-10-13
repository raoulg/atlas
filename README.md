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

**A student's script** — 6 questions

<a href="https://raoulg.github.io/atlas/showcase/python-sloppy.html"><img src="docs/img/showcase/python-sloppy.png" width="59%" alt="Atlas's picture of the student's script: the web and the files hang off two functions; nothing passes between the steps"></a>

**Typed, in small steps** — no questions

<a href="https://raoulg.github.io/atlas/showcase/python-clean.html"><img src="docs/img/showcase/python-clean.png" width="100%" alt="Atlas's picture of the clean pipeline: a DataFrame passes from load to smooth to Trend.save"></a>

The same pipeline (fetch a CSV, keep it, clean it, smooth it, draw a figure), the same places at
its edges, the same figure out. [Click through it, and the same program in Rust.](https://raoulg.github.io/atlas/)

## The mathematics of your code

| In the code | In Atlas |
| --- | --- |
| where data rests: a value, a file, a table, a URL, the screen | a **place** |
| a function | an **arrow** from what it takes to what it gives back or writes |
| one call after another | a **route** from an entry point to its places |
| a base class and its kinds | a **family** that folds and opens |

No language owns this. A Python function and a Rust function that read the same file and write the
same table are the same arrow, so a port can be laid over its original.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/raoulg/atlas/main/install.sh | sh
```

Windows: `irm https://raw.githubusercontent.com/raoulg/atlas/main/install.ps1 | iex`. From source:
`cargo install --locked --git https://github.com/raoulg/atlas atlas`.

```bash
atlas view      # draw the repository you are in
atlas check     # where the code and ten design principles disagree
atlas changes   # what your edit did to the routes
atlas mcp       # the same picture for your coding agent
```

## More

- [**See it**](https://raoulg.github.io/atlas/): the examples, side by side, to click through.
- [**For agents**](docs/llms.txt): the design, the principles and the guide, written to be read in one go.

Alpha: what Atlas reads and draws changes from one minor version to the next. Built by Raoul Grouls
with [Claude Code](https://claude.com/claude-code). [CC BY-NC-SA 4.0](LICENSE).
