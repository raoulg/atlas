mod history;
mod http;
mod log;
mod mcp;
mod serve;
mod shared;

use std::path::{Path, PathBuf};

use anyhow::Result;
use atlas_engine::changes::{self, Named};
use atlas_engine::{parts, principles, remote, Graph, Options};
use clap::builder::styling::{AnsiColor, Effects, Style, Styles};
use clap::{Parser, Subcommand};

/// The version and the commit this build is from: `0.1.0 (a5e777d)`, with `+` after a commit
/// whose tree had edits.
static VERSION: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| match env!("ATLAS_COMMIT") {
    "" => env!("CARGO_PKG_VERSION").to_string(),
    c => format!("{} ({c})", env!("CARGO_PKG_VERSION")),
});

/// The logo's colours in the terminal's own palette: structure in petrol (cyan), commands in ink.
const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .literal(Style::new().effects(Effects::BOLD))
    .placeholder(AnsiColor::Cyan.on_default())
    .valid(AnsiColor::Cyan.on_default())
    .invalid(AnsiColor::Red.on_default().effects(Effects::BOLD))
    .error(AnsiColor::Red.on_default().effects(Effects::BOLD));

/// The logo's A, over the window of Rule 30 the logo shows (generations 37 to 49, columns 4 to 16
/// from the seed): `a` a cell of the route, light grey; `P` a place it joins, petrol; `R` the
/// place where it ends, vermilion.
const LOGO_A: [&str; 13] = [
    ".............",
    ".............",
    "......P......",
    "......aa.....",
    ".....a.a.....",
    ".....a..a....",
    "....a...a....",
    "....a....a...",
    "...PaaaaaP...",
    "...a......a..",
    "..P.......R..",
    ".............",
    ".............",
];

/// The logo, wider: Rule 30 grown from one cell, the same generations, with the A where the logo
/// has it. Two cells share a character (`▀`, `▄`, `█`), so a cell is about square.
fn banner() -> String {
    const FIRST: usize = 37;
    const ROWS: usize = 13;
    // the columns drawn, from the seed: the logo's 4..=16 in the middle
    const LEFT: i32 = -14;
    const COLS: i32 = 49;
    let span = (FIRST + ROWS) as i32;
    let width = (2 * span + 1) as usize;
    let mut row = vec![false; width];
    row[span as usize] = true;
    let mut window = vec![];
    for generation in 0..FIRST + ROWS {
        if generation >= FIRST {
            window.push(row.clone());
        }
        // Rule 30: a cell lives when its left neighbour differs from (itself or its right neighbour)
        row = (0..width).map(|i| (i > 0 && row[i - 1]) ^ (row[i] || (i + 1 < width && row[i + 1]))).collect();
    }
    let rule = AnsiColor::BrightBlack;
    let colour = |r: usize, c: i32| -> Option<AnsiColor> {
        let x = c - 4;
        if (0..13).contains(&x) {
            match LOGO_A[r].as_bytes()[x as usize] {
                b'a' => return Some(AnsiColor::White),
                b'P' => return Some(AnsiColor::Cyan),
                b'R' => return Some(AnsiColor::Red),
                _ => {}
            }
        }
        window[r][(span + c) as usize].then_some(rule)
    };
    let (dim, bold) = (Style::new().effects(Effects::DIMMED), Style::new().effects(Effects::BOLD));
    let mut lines = vec![];
    for pair in (0..ROWS).step_by(2) {
        let mut line = String::new();
        for c in LEFT..LEFT + COLS {
            let (up, down) = (colour(pair, c), if pair + 1 < ROWS { colour(pair + 1, c) } else { None });
            let (glyph, style) = match (up, down) {
                (None, None) => (' ', Style::new()),
                (Some(u), None) => ('▀', u.on_default()),
                (None, Some(d)) => ('▄', d.on_default()),
                (Some(u), Some(d)) if u == d => ('█', u.on_default()),
                (Some(u), Some(d)) => ('▀', u.on(d)),
            };
            line += &format!("{style}{glyph}{style:#}");
        }
        lines.push(line);
    }
    lines[2] += &format!("   {bold}atlas{bold:#}");
    lines[3] += &format!("   {dim}{}{dim:#}", VERSION.as_str());
    lines.join("\n")
}

fn examples() -> String {
    let (head, cmd, dim) = (STYLES.get_header(), STYLES.get_literal(), Style::new().effects(Effects::DIMMED));
    let rows = [
        ("atlas serve", "the repository you are in, as a page to work in with your agent"),
        ("atlas view", "the same picture as one HTML file, opened in the browser"),
        ("atlas view ../api ../web", "two repositories as parts of one picture"),
        ("atlas view notebooks/explore.ipynb", "one file, read with its project, opened on its route"),
        ("atlas view https://github.com/o/r/tree/main/src/pkg", "one folder of a repository on GitHub"),
        ("atlas check", "where the code and the design principles disagree"),
        ("atlas changes", "what your edits did to the routes, their places and the findings"),
        ("atlas changes v1..main", "what changed between two revisions"),
        ("atlas history", "the history in the browser: pick two commits, see one picture of both"),
        ("claude mcp add atlas -- atlas mcp", "give Claude Code the same picture"),
    ];
    let mut s = format!("{head}Examples:{head:#}\n");
    for (c, what) in rows {
        // a command longer than its column gets the line to itself, and its words the next one
        match c.chars().count() {
            n if n < 36 => s += &format!("  {cmd}{c:<36}{cmd:#}{dim}{what}{dim:#}\n"),
            _ => s += &format!("  {cmd}{c}{cmd:#}\n  {:<36}{dim}{what}{dim:#}\n", ""),
        }
    }
    s
}

#[derive(Parser)]
#[command(
    name = "atlas",
    version = VERSION.as_str(),
    about = "A mathematical panorama of your software code.",
    before_help = banner(),
    after_help = examples(),
    styles = STYLES
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
    /// How far to read into the libraries the code calls: 0 not at all, 1 the modules its calls go into, 2 also the modules of those libraries they call, and so on
    #[arg(long, global = true, default_value_t = 1, value_name = "ROUNDS")]
    libraries: usize,
    /// Read everything anew: use nothing earlier readings kept, and keep nothing
    #[arg(long, global = true)]
    no_cache: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// Serve a repository's page on this machine, drawn anew when its code changes and shared with
    /// an agent: what the page shows, where the agent points it, and the notes written on its boxes
    /// and lines (`atlas mcp`: `shown`, `look`, `notes`, `resolve`)
    Serve {
        /// Repository root; default: the current directory.
        repo: Option<PathBuf>,
        /// The port to serve at, on 127.0.0.1; default: the repository's own, the same each time so
        /// that a link to a place in it keeps working, else a free one; 0: a free one.
        #[arg(long)]
        port: Option<u16>,
        /// Serve without opening a browser.
        #[arg(long)]
        no_open: bool,
    },
    /// Write the picture of one or more repositories as one HTML file and open it: what `atlas serve`
    /// exports.
    View {
        /// Repository roots, files or git URLs (a `…/tree/<ref>/<folder>` URL names one folder; a file is read with its project, even where git ignores it, and the page opens on its route); default:
        /// the current directory. Several become parts of one page.
        repos: Vec<PathBuf>,
        /// Where to write the page; default: `.atlas/<project>.html` at the top of the git
        /// repository it is in.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Write the page without opening a browser.
        #[arg(long)]
        no_open: bool,
        /// Draw this git revision and the working tree as one picture, marking what was added,
        /// removed and changed since it (one repository).
        #[arg(long)]
        since: Option<String>,
        /// With `--since`: compare with this revision instead of the working tree.
        #[arg(long, requires = "since")]
        until: Option<String>,
        /// Open the page in this view, as JSON: `{"route": <entry id>, "open": {"functions": [<id>, ..]}}`, and
        /// `kind`, `part`, `focus`, and the opened `parts`, `units`, `steps`, `families`, `places`, `dirs`, as the page's address writes them.
        #[arg(long, value_name = "JSON")]
        view: Option<String>,
    },
    /// List where the code and the design principles disagree, each with a question.
    Check {
        /// Repository root or git URL; default: the current directory.
        repo: Option<PathBuf>,
        /// Print the findings as JSON.
        #[arg(long)]
        json: bool,
    },
    /// What the working tree changes against a git revision, or one revision against another:
    /// routes, their steps and places, findings.
    Changes {
        /// The revision to compare with (`HEAD~2`), or two revisions (`v1..main`); default: HEAD.
        rev: Option<String>,
        /// Repository root; default: the current directory.
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Print the changes as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Serve Atlas to a coding agent over MCP (stdio): principles, check, explain, routes, route, view, changes, and with `atlas serve`, shown, look, notes, answer.
    Mcp,
    /// Serve a repository's history on this machine and open it in the browser: its commits, and
    /// the picture of one of them or of two as one.
    History {
        /// Repository root; default: the current directory.
        repo: Option<PathBuf>,
        /// The port to serve at, on 127.0.0.1; default: a free one.
        #[arg(long, default_value_t = 0)]
        port: u16,
        /// Serve without opening a browser.
        #[arg(long)]
        no_open: bool,
    },
    /// Write a repository's graph as JSON.
    Analyze {
        /// Repository root, a package directory, or a git URL.
        repo: PathBuf,
        /// Package directory, when discovery picks the wrong one.
        #[arg(long)]
        package: Option<PathBuf>,
        /// Where to write the graph; default: `.atlas/graph.json` at the top of the git repository
        /// it is in.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
}

/// The viewer, baked into the binary, with its fonts: the layout of the graph it is handed.
const VIEWER: &str = include_str!("../../viewer/index.html");
const FONTS: &str = include_str!("../../viewer/fonts.css");

/// What a page draws: one graph, what the principles ask of it, and, where two readings are drawn
/// as one, how its things stand to them.
pub(crate) struct Drawing {
    graph: Graph,
    findings: Vec<principles::Finding>,
    diff: Option<serde_json::Value>,
}

/// One reading as a drawing: its parts, each read and checked in its own language, as one graph.
pub(crate) fn single(repos: &[PathBuf], graphs: Vec<Graph>) -> Drawing {
    let findings = graphs.iter().flat_map(principles::check).collect();
    Drawing { graph: parts::merged(repos, graphs), findings, diff: None }
}

/// Two readings of `repo` as one drawing: the union of the parts read at A and at B, the findings
/// of B, and how each thing of the union stands to the two, with the revisions they were read at
/// and how many findings B raises and resolves.
pub(crate) fn union(repo: &Path, a: (Named, Vec<Graph>), b: (Named, Vec<Graph>)) -> Result<Drawing> {
    let name = std::fs::canonicalize(repo)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "repo".into());
    let (fa, fb): (Vec<principles::Finding>, Vec<principles::Finding>) =
        (a.1.iter().flat_map(principles::check).collect(), b.1.iter().flat_map(principles::check).collect());
    let (raised, resolved) = changes::raised(&fa, &fb);
    let (graph, d) = atlas_engine::diff::union(&name, a.1, b.1);
    let mut diff = serde_json::to_value(&d)?;
    diff["a"] = serde_json::to_value(&a.0)?;
    diff["b"] = serde_json::to_value(&b.0)?;
    diff["findings"] = serde_json::json!({ "raised": raised.len(), "resolved": resolved.len() });
    Ok(Drawing { graph, findings: fb, diff: Some(diff) })
}

/// Revision `a` of `repo` and revision `b` (the working tree when there is none) as one drawing.
fn compared(repo: &Path, a: &str, b: Option<&str>, opts: &Options) -> Result<Drawing> {
    let (_, named_a) = changes::resolve(repo, a)?;
    let named_b = b.map(|b| changes::resolve(repo, b).map(|(_, n)| n)).transpose()?.unwrap_or_else(changes::worktree);
    let before = changes::parts_at(repo, Some(a), opts)?;
    union(repo, (named_a, before), (named_b, changes::parts_at(repo, b, opts)?))
}

/// What `atlas view` draws: the repositories as they are, or one repository since a revision.
fn reading(repos: &[PathBuf], opts: &Options, since: Option<&str>, until: Option<&str>) -> Result<Drawing> {
    match (since, repos) {
        (None, _) => Ok(single(repos, parts::analyze_parts(repos, opts)?)),
        (Some(a), [repo]) => compared(repo, a, until, opts),
        (Some(_), _) => anyhow::bail!("--since compares one repository with its history"),
    }
}

/// The viewer holding one drawing: its graph, the flow its routes are read from, the findings at
/// the nodes they are about, how its things stand to two readings when it is their union, and
/// what else the page is handed (`open`, `view`).
pub(crate) fn viewer(d: &Drawing, extra: serde_json::Map<String, serde_json::Value>) -> Result<String> {
    let mut page = serde_json::to_value(&d.graph)?;
    // what each route runs and what each step takes and gives: the flow the agent reads its routes from
    page["flow"] = serde_json::to_value(atlas_engine::flow::of(&d.graph))?;
    // what the principles ask, each at the node it is about, where the page marks it; the merged
    // graph keeps every part's nodes, so each finding is about a node the picture knows
    page["findings"] = serde_json::to_value(&d.findings)?;
    if let Some(diff) = &d.diff {
        page["diff"] = diff.clone();
    }
    for (k, v) in extra {
        page[k] = v;
    }
    let mut projects = serde_json::Map::new();
    projects.insert(d.graph.root.clone(), page);
    let payload = serde_json::to_string(&serde_json::json!({ "projects": projects, "atlas": VERSION.as_str() }))?.replace("</", "<\\/");
    Ok(VIEWER.replacen("/*__FONTS__*/", FONTS, 1).replacen("/*__DATA__*/null", &payload, 1))
}

/// The page of one or more repositories, or of one repository since a revision (`since`, until
/// `until` or the working tree), drawn in `shown` when it is given (a view: its route, kind,
/// part, focus and opened sets), else on the route of a file named on the command line. Gives the
/// page and the name of its project.
pub(crate) fn page(
    repos: &[PathBuf],
    opts: &Options,
    since: Option<&str>,
    until: Option<&str>,
    shown: Option<serde_json::Value>,
) -> Result<(String, String, String)> {
    let d = reading(repos, opts, since, until)?;
    let g = &d.graph;
    // what the page draws, in a line
    let parts: Vec<&str> = g.parts.iter().map(|p| p.id.as_str()).collect();
    let entries = g.nodes.iter().filter(|n| n.entry.is_some()).count();
    let summary = format!("{} · {} · {} modules · {entries} entries", g.root, parts.join(" + "), g.stats.modules);
    let mut extra = serde_json::Map::new();
    // a file named on the command line opens on its route
    if let Some(n) = g.nodes.iter().find(|n| n.entry.is_some() && std::fs::canonicalize(&n.file).map_or(false, |f| opts.named.contains(&f)))
    {
        extra.insert("open".into(), serde_json::Value::String(n.id.clone()));
    }
    if let Some(v) = shown {
        extra.insert("view".into(), v);
    }
    Ok((g.root.clone(), viewer(&d, extra)?, summary))
}

/// The page of the working tree of `repo`, served: it tells its server what it shows, asks it where
/// an agent points, keeps notes through it, and knows its drawing as `drawn`, to follow the next.
pub(crate) fn served_page(repo: &Path, opts: &Options, drawn: &str) -> Result<(String, String)> {
    let d = reading(&[repo.to_path_buf()], opts, None, None)?;
    let g = &d.graph;
    let parts: Vec<&str> = g.parts.iter().map(|p| p.id.as_str()).collect();
    let summary = format!("{} · {} · {} modules", g.root, parts.join(" + "), g.stats.modules);
    let mut extra = serde_json::Map::new();
    extra.insert("served".into(), serde_json::Value::Bool(true));
    extra.insert("drawn".into(), serde_json::Value::String(drawn.into()));
    Ok((viewer(&d, extra)?, summary))
}

/// Open a page or an address in the browser the system names.
pub(crate) fn open(target: &std::ffi::OsStr) {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open").arg(target).status();
    #[cfg(target_os = "linux")]
    let status = std::process::Command::new("xdg-open").arg(target).status();
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("cmd").args(["/C", "start", ""]).arg(target).status();
    if let Err(e) = status {
        eprintln!("could not open a browser ({e}); open {} yourself", target.to_string_lossy());
    }
}

/// Where Atlas puts what it makes when no place is asked for: `.atlas/` at the top of the git
/// repository `dir` is in (else in `dir`), where whoever works there, a person or an agent, finds
/// it. The folder ignores itself, so git never sees it. A clone is gone when the command ends, so
/// what is made of one goes to a temporary folder.
pub(crate) fn made(dir: Option<&Path>, file: &str) -> PathBuf {
    let Some(dir) = dir else {
        let tmp = std::env::temp_dir().join("atlas");
        let _ = std::fs::create_dir_all(&tmp);
        return tmp.join(file);
    };
    let dir = atlas_engine::repo::normal(&std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf()));
    let top = dir.ancestors().find(|a| a.join(".git").exists()).unwrap_or(&dir);
    let home = top.join(".atlas");
    if std::fs::create_dir_all(&home).is_ok() && !home.join(".gitignore").exists() {
        let _ = std::fs::write(home.join(".gitignore"), "# What atlas makes: pages and graphs, never committed.\n*\n");
    }
    home.join(file)
}

/// The page of `repos`, written to `out`, else beside `home`, the folder of the first of them
/// when it is not a clone.
fn view(
    repos: &[PathBuf],
    opts: &Options,
    (out, home): (Option<PathBuf>, Option<&Path>),
    shown_in_browser: bool,
    range: (Option<&str>, Option<&str>),
    shown: Option<serde_json::Value>,
) -> Result<()> {
    let (name, html, summary) = page(repos, opts, range.0, range.1, shown)?;
    let out = out.unwrap_or_else(|| made(home, &format!("{name}.html")));
    std::fs::write(&out, html)?;
    eprintln!("{summary} · wrote {}", out.display());
    if shown_in_browser {
        open(out.as_os_str());
    }
    Ok(())
}

/// A repository argument as a folder on disk: a path as it is, a URL cloned for as long as the
/// command runs.
/// A file named instead of a folder is read as part of its project: the nearest folder above it
/// with a manifest, else its own folder. It is read even where git ignores it, and the page opens
/// on its route.
fn project_of(file: &std::path::Path) -> PathBuf {
    let file = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let dir = file.parent().unwrap_or(&file);
    let root = dir.ancestors().find(|d| ["pyproject.toml", "Cargo.toml", "package.json"].iter().any(|m| d.join(m).exists())).unwrap_or(dir);
    // the project is written from where atlas runs, as a folder named on the command line is
    let here = std::env::current_dir().ok().and_then(|c| std::fs::canonicalize(c).ok());
    match here.as_deref().and_then(|h| root.strip_prefix(h).ok()) {
        Some(r) if r.as_os_str().is_empty() => PathBuf::from("."),
        Some(r) => PathBuf::from(".").join(r),
        None => root.to_path_buf(),
    }
}

/// A file among the arguments is named: `opts` reads it as one of its project's files.
fn local(arg: PathBuf, opts: &mut Options) -> Result<(PathBuf, Option<remote::Clone>)> {
    let spec = arg.to_string_lossy().to_string();
    if !remote::is_url(&spec) {
        if arg.is_file() {
            opts.named.extend(std::fs::canonicalize(&arg).ok());
            return Ok((project_of(&arg), None));
        }
        return Ok((arg, None));
    }
    eprintln!("cloning {spec}");
    let c = remote::clone(&spec)?;
    Ok((c.dir.clone(), Some(c)))
}

/// The findings, grouped by principle, each ending in its question, and below them how many
/// each part has; exit status 1 when there is anything to ask.
fn check(repo: &std::path::Path, opts: &Options, json: bool) -> Result<bool> {
    // each part in its own language: what a Python package declares, a crate's compiler checks
    let parts: Vec<(String, Vec<principles::Finding>)> = parts::analyze_parts(&[repo.to_path_buf()], opts)?
        .into_iter()
        .map(|g| {
            let f = principles::check(&g);
            (g.root, f)
        })
        .collect();
    if json {
        for (root, findings) in &parts {
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "root": root, "findings": findings }))?);
        }
    } else {
        let look = look();
        for (i, (root, findings)) in parts.iter().enumerate() {
            print!("{}{}", if i > 0 { "\n" } else { "" }, principles::report(root, findings, &look));
        }
        let counted: Vec<(&str, &[principles::Finding])> = parts.iter().map(|(r, f)| (r.as_str(), f.as_slice())).collect();
        print!("{}", principles::summary(&counted, &principles::PRINCIPLES.iter().collect::<Vec<_>>(), &look));
    }
    Ok(parts.iter().all(|(_, f)| f.is_empty()))
}

/// How a report looks here: in a terminal, the logo's colours (principles in petrol, a question's
/// mark in vermilion) at the terminal's width, up to a hundred columns; elsewhere, or with
/// `NO_COLOR`, plain text.
fn look() -> principles::Look {
    use principles::Ink;
    use std::io::IsTerminal;
    let terminal = std::io::stdout().is_terminal();
    let mut look = principles::Look::plain();
    look.width = terminal.then(columns).flatten().map(|c| c.min(100));
    if terminal && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()) {
        look.ink = |ink, s| {
            let style = match ink {
                Ink::Part | Ink::Count | Ink::Asks => Style::new().effects(Effects::BOLD),
                Ink::Principle => AnsiColor::Cyan.on_default().effects(Effects::BOLD),
                Ink::At | Ink::Bar => AnsiColor::Cyan.on_default(),
                Ink::Text => Style::new(),
                Ink::Why => Style::new().effects(Effects::DIMMED.insert(Effects::ITALIC)),
                Ink::Evidence | Ink::Quiet => Style::new().effects(Effects::DIMMED),
                Ink::Ask => AnsiColor::Red.on_default().effects(Effects::BOLD),
            };
            format!("{style}{s}{style:#}")
        };
    }
    look
}

/// The terminal's width in columns.
#[cfg(unix)]
fn columns() -> Option<usize> {
    let mut size: libc::winsize = unsafe { std::mem::zeroed() };
    // SAFETY: TIOCGWINSZ writes a winsize for the descriptor and nothing else
    let ok = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut size) } == 0;
    (ok && size.ws_col > 0).then_some(size.ws_col as usize)
}

#[cfg(not(unix))]
fn columns() -> Option<usize> {
    None
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cache = if cli.no_cache { None } else { atlas_engine::cache::Cache::new(VERSION.as_str()) };
    let mut opts = Options { libraries: cli.libraries, cache, ..Options::default() };
    match cli.cmd {
        Cmd::View { repos, out, no_open, since, until, view: shown } => {
            let repos = if repos.is_empty() { vec![std::env::current_dir()?] } else { repos };
            let shown = shown
                .map(|v| serde_json::from_str::<serde_json::Value>(&v))
                .transpose()
                .map_err(|e| anyhow::anyhow!("--view is not JSON: {e}"))?;
            let (repos, clones): (Vec<PathBuf>, Vec<Option<remote::Clone>>) =
                repos.into_iter().map(|r| local(r, &mut opts)).collect::<Result<Vec<_>>>()?.into_iter().unzip();
            let home = clones[0].is_none().then(|| repos[0].as_path());
            view(&repos, &opts, (out, home), !no_open, (since.as_deref(), until.as_deref()), shown)?
        }
        Cmd::Mcp => mcp::serve(&opts)?,
        Cmd::History { repo, port, no_open } => {
            let repo = match repo {
                Some(r) => r,
                None => std::env::current_dir()?,
            };
            // this machine only: the page shows the repository's code
            history::serve(&repo, std::net::SocketAddr::from(([127, 0, 0, 1], port)), &opts, !no_open)?
        }
        Cmd::Serve { repo, port, no_open } => {
            let repo = match repo {
                Some(r) => r,
                None => std::env::current_dir()?,
            };
            serve::serve(&repo, port, &opts, !no_open)?
        }
        Cmd::Changes { rev, repo, json } => {
            let repo = match repo {
                Some(r) => r,
                None => std::env::current_dir()?,
            };
            let (a, b) = changes::range(rev.as_deref().unwrap_or("HEAD"));
            let c = changes::compare(&repo, a, b, &opts)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&c)?);
            } else {
                print!("{}", atlas_engine::changes::report(&c));
            }
        }
        Cmd::Check { repo, json } => {
            let (repo, _clone) = local(
                match repo {
                    Some(r) => r,
                    None => std::env::current_dir()?,
                },
                &mut opts,
            )?;
            if !check(&repo, &opts, json)? {
                std::process::exit(1);
            }
        }
        Cmd::Analyze { repo, package, out } => {
            let (repo, clone) = local(repo, &mut opts)?;
            let out = out.unwrap_or_else(|| made(clone.is_none().then_some(repo.as_path()), "graph.json"));
            let g = match package {
                Some(p) => atlas_engine::analyze(&repo, &Options { package: Some(p), ..opts })?,
                None => parts::analyze_repo(&[repo.clone()], &opts)?,
            };
            std::fs::write(&out, serde_json::to_string(&g)?)?;
            let s = &g.stats;
            eprintln!("{} · {} modules · nodes {:?} · edges {:?}", g.root, s.modules, s.nodes, s.edges);
            eprintln!(
                "unresolved {} (top: {})",
                s.unresolved_calls_total,
                s.unresolved_calls_top.iter().take(8).map(|(k, v)| format!("{k}×{v}")).collect::<Vec<_>>().join(", ")
            );
            let u = &s.unresolved_calls;
            eprintln!(
                "  builtin {} · library {} · local {} · unknown {} ({:.1}% of the calls not followed)",
                u.builtin,
                u.library,
                u.local,
                u.unknown,
                100.0 * s.unknown_share()
            );
            eprintln!("entries {} · make targets {}", g.nodes.iter().filter(|n| n.entry.is_some()).count(), g.make.len());
            eprintln!("wrote {}", out.display());
        }
    }
    Ok(())
}
