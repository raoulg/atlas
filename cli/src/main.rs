mod history;
mod home;
mod http;
mod log;
mod mcp;
mod page;
mod serve;
mod shared;
mod update;
mod version;

use std::path::{Path, PathBuf};

use anyhow::Result;
use atlas_engine::changes;
use atlas_engine::sight::Sight;
use atlas_engine::{parts, principles, remote, Graph, Options};
use clap::builder::styling::{AnsiColor, Effects, Style, Styles};
use clap::{Parser, Subcommand};

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
    lines[3] += &format!("   {dim}{}{dim:#}", version::VERSION.as_str());
    lines.join("\n")
}

/// The help's layout, clap's own with the banner on top. The banner is written into the template,
/// not handed over as text before the help: the help is wrapped to the terminal, and the banner is a
/// picture whose lines must stay whole.
fn help_template() -> String {
    format!("{}\n\n{{about-with-newline}}\n{{usage-heading}} {{usage}}\n\n{{all-args}}{{after-help}}", banner())
}

fn examples() -> String {
    let (head, cmd, dim) = (STYLES.get_header(), STYLES.get_literal(), Style::new().effects(Effects::DIMMED));
    let rows = [
        ("atlas serve", "a page to work in with your agent"),
        ("atlas view", "the same picture as one HTML file"),
        ("atlas view ../api ../web", "two repositories as parts of one picture"),
        ("atlas view notebooks/explore.ipynb", "one file, opened on its route"),
        ("atlas view https://github.com/o/r/tree/main/src/pkg", "one folder of a repository on GitHub"),
        ("atlas check", "where code and principles disagree"),
        ("atlas revelation", "what is left to build"),
        ("atlas changes", "what your edits did to the routes"),
        ("atlas changes v1..main", "what changed between two revisions"),
        ("atlas history", "pick two commits, see them as one picture"),
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
    version = version::VERSION.as_str(),
    about = "A mathematical panorama of your software code.",
    help_template = help_template(),
    after_help = examples(),
    styles = STYLES
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
    /// How far to read into the libraries the code calls
    ///
    /// 0 not at all, 1 the modules its calls go into, 2 also the modules of those libraries they
    /// call, and so on.
    #[arg(long, global = true, default_value_t = 1, value_name = "ROUNDS", help_heading = "Global options")]
    libraries: usize,
    /// Read everything anew: use nothing earlier readings kept, and keep nothing
    #[arg(long, global = true, help_heading = "Global options")]
    no_cache: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// The picture of a repository as a page to work in with your agent
    ///
    /// Serves a repository's page on this machine, drawn anew when its code changes and shared with
    /// an agent: what the page shows, where the agent points it, and the notes written on its boxes
    /// and lines (`atlas mcp`: `shown`, `look`, `notes`, `resolve`).
    Serve {
        /// Repository root; default: the current directory.
        repo: Option<PathBuf>,
        /// The port to serve at, on 127.0.0.1
        ///
        /// Default: the repository's own, the same each time so that a link to a place in it keeps
        /// working, else a free one; 0: a free one.
        #[arg(long)]
        port: Option<u16>,
        /// Serve without opening a browser.
        #[arg(long)]
        no_open: bool,
    },
    /// The same picture as one HTML file, to keep or send
    ///
    /// Writes the picture of one or more repositories as one HTML file and opens it: what `atlas
    /// serve` exports.
    View {
        /// Repository roots, files or git URLs; default: the current directory
        ///
        /// Several become parts of one page. A `…/tree/<ref>/<folder>` URL names one folder; a file
        /// is read with its project, even where git ignores it, and the page opens on its route.
        repos: Vec<PathBuf>,
        /// Where to write the page; default: `.atlas/<project>.html` at the top of the repository
        #[arg(long)]
        out: Option<PathBuf>,
        /// Write the page without opening a browser.
        #[arg(long)]
        no_open: bool,
        /// Draw this revision and the working tree as one picture
        ///
        /// What was added, removed and changed since it is marked (one repository).
        #[arg(long)]
        since: Option<String>,
        /// With `--since`: compare with this revision instead of the working tree.
        #[arg(long, requires = "since")]
        until: Option<String>,
        /// Open the page in this view, as JSON: `{"route": <entry id>}`
        ///
        /// Also `"open": {"functions": [<id>, ..]}`, and `kind`, `part`, `focus`, and the opened
        /// `parts`, `units`, `steps`, `families`, `places`, `dirs`, as the page's address writes them.
        #[arg(long, value_name = "JSON")]
        view: Option<Sight>,
    },
    /// Where the code and the design principles disagree, each with a question
    Check {
        /// Repository root or git URL; default: the current directory.
        repo: Option<PathBuf>,
        /// Print the findings as JSON.
        #[arg(long)]
        json: bool,
    },
    /// The sketch code kept in the tree, and what is left to build
    ///
    /// Every prophecy in the tree (sketch code marked as not built yet), what it replaces and what
    /// is left of it.
    Revelation {
        /// Repository root or git URL; default: the current directory.
        repo: Option<PathBuf>,
        /// Print the standings as JSON.
        #[arg(long)]
        json: bool,
        /// Only the sealed prophecies, with the state of the issue each waits on
        ///
        /// The state is asked of GitHub with `gh`.
        #[arg(long)]
        seals: bool,
    },
    /// What your edits did to the routes, their places and the findings
    ///
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
    /// The picture for your coding agent, over MCP
    ///
    /// Serves Atlas to a coding agent over MCP (stdio): principles, check, explain, routes, route,
    /// view, changes, and with `atlas serve`, shown, look, notes, answer.
    Mcp,
    /// The history in your browser: pick two commits, see them as one picture
    ///
    /// Serves a repository's history on this machine and opens it in the browser: its commits, and
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
    /// A repository's graph as JSON
    Analyze {
        /// Repository root, a package directory, or a git URL.
        repo: PathBuf,
        /// Package directory, when discovery picks the wrong one.
        #[arg(long)]
        package: Option<PathBuf>,
        /// Where to write the graph; default: `.atlas/graph.json` at the top of the repository
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
}

/// The page of `repos`, or of one of them `since` a revision (`until` another), written to `out`,
/// else beside `home`, the folder of the first of them when it is not a clone. It opens in `sight`,
/// and on the route of a file named on the command line.
fn view(
    repos: &[PathBuf],
    opts: &Options,
    (out, home): (Option<PathBuf>, Option<&Path>),
    shown_in_browser: bool,
    (since, until): (Option<&str>, Option<&str>),
    sight: Option<Sight>,
) -> Result<()> {
    let read = page::reading(repos, opts, since, until)?;
    let findings = read.findings();
    let (graph, compared) = read.drawn();
    let flow = atlas_engine::flow::of(&graph);
    let opens = page::Opens { route: named_route(&graph, &opts.named), sight, served: None };
    let html = page::Html::of(&graph, &flow, &findings, compared.as_ref(), &opens)?;
    let out = out.unwrap_or_else(|| home::made(home, &format!("{}.html", graph.root)));
    html.write(&out)?;
    eprintln!("{} · wrote {}", page::summary(&graph), out.display());
    if shown_in_browser {
        page::open(out.as_os_str());
    }
    Ok(())
}

/// The route of a file named on the command line: the entry that lies in one of the `named`, by its id.
fn named_route(graph: &Graph, named: &[PathBuf]) -> Option<String> {
    let lies_in = |file: &str| std::fs::canonicalize(file).is_ok_and(|f| named.contains(&f));
    graph.nodes.iter().find(|n| n.entry.is_some() && lies_in(&n.file)).map(|n| n.id.clone())
}

/// A repository argument as a folder on disk: a path as it is, a URL cloned for as long as the
/// command runs.
/// A file named instead of a folder is read as part of its project: the nearest folder above it
/// with a manifest, else its own folder. It is read even where git ignores it, and the page opens
/// on its route.
fn project_of(file: &std::path::Path) -> PathBuf {
    let file = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let dir = file.parent().unwrap_or(&file);
    let manifests = atlas_engine::language::manifests();
    let root = dir.ancestors().find(|d| manifests.iter().any(|m| d.join(m).exists())).unwrap_or(dir);
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
/// each part has; as JSON, one document that holds every part (`{"parts": [{"root", "findings"}]}`).
/// Exit status 1 when there is anything to ask.
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
        let reports: Vec<serde_json::Value> =
            parts.iter().map(|(root, findings)| serde_json::json!({ "root": root, "findings": findings })).collect();
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "parts": reports }))?);
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
    // a person's commands say when a newer build is out; the agent's and the graph's do not
    let update = if matches!(cli.cmd, Cmd::Mcp | Cmd::Analyze { .. }) { None } else { Some(update::begin()) };
    let cache = if cli.no_cache { None } else { atlas_engine::cache::Cache::new(version::VERSION.as_str()) };
    let mut opts = Options { libraries: cli.libraries, cache, ..Options::default() };
    match cli.cmd {
        Cmd::View { repos, out, no_open, since, until, view: sight } => {
            let repos = if repos.is_empty() { vec![std::env::current_dir()?] } else { repos };
            let (repos, clones): (Vec<PathBuf>, Vec<Option<remote::Clone>>) =
                repos.into_iter().map(|r| local(r, &mut opts)).collect::<Result<Vec<_>>>()?.into_iter().unzip();
            let home = clones[0].is_none().then(|| repos[0].as_path());
            view(&repos, &opts, (out, home), !no_open, (since.as_deref(), until.as_deref()), sight)?
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
        Cmd::Revelation { repo, json, seals } => {
            let (repo, _clone) = local(
                match repo {
                    Some(r) => r,
                    None => std::env::current_dir()?,
                },
                &mut opts,
            )?;
            for (i, g) in parts::analyze_parts(&[repo.clone()], &opts)?.iter().enumerate() {
                let stands = atlas_engine::revelation::revelation(g);
                let sealed = atlas_engine::revelation::seals(&stands, &|issue| shared::issue_state(&repo, issue));
                let sep = if i > 0 { "\n" } else { "" };
                match (json, seals) {
                    (true, true) => println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "root": g.root, "seals": sealed }))?),
                    (true, false) => {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({ "root": g.root, "prophecies": stands, "seals": sealed }))?
                        )
                    }
                    (false, true) => print!("{sep}{}", atlas_engine::revelation::seals_report(&g.root, &sealed)),
                    (false, false) => print!("{sep}{}", atlas_engine::revelation::report(&g.root, &stands, &sealed)),
                }
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
            let out = out.unwrap_or_else(|| home::made(clone.is_none().then_some(repo.as_path()), "graph.json"));
            let g = match package {
                Some(p) => atlas_engine::analyze(&repo, &Options { package: Some(p), ..opts })?,
                None => parts::analyze_repo(&[repo.clone()], &opts)?,
            };
            if let Some(dir) = out.parent() {
                std::fs::create_dir_all(dir)?;
            }
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
    if let Some(update) = update {
        update.finish();
    }
    Ok(())
}
