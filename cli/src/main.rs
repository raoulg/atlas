mod mcp;

use std::path::PathBuf;

use anyhow::Result;
use atlas_engine::{parts, principles, remote};
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

/// The logo in a line: places joined by a route that ends in vermilion, where its data rests.
fn banner() -> String {
    let (place, end) = (AnsiColor::Cyan.on_default(), AnsiColor::Red.on_default());
    let (dim, bold) = (Style::new().effects(Effects::DIMMED), Style::new().effects(Effects::BOLD));
    format!(
        "{place}■{place:#}{dim}──▶{dim:#}{place}■{place:#}{dim}──▶{dim:#}{end}■{end:#}  {bold}atlas{bold:#} {dim}{}{dim:#}",
        VERSION.as_str()
    )
}

fn examples() -> String {
    let (head, cmd, dim) = (STYLES.get_header(), STYLES.get_literal(), Style::new().effects(Effects::DIMMED));
    let rows = [
        ("atlas view", "draw the repository you are in and open it in the browser"),
        ("atlas view ../api ../web", "two repositories as parts of one picture"),
        ("atlas view https://github.com/o/r/tree/main/src/pkg", "one folder of a repository on GitHub"),
        ("atlas check", "where the code and the design principles disagree"),
        ("atlas changes", "what your edits did to the routes, their places and the findings"),
        ("claude mcp add atlas -- atlas mcp", "give Claude Code the same picture"),
    ];
    let mut s = format!("{head}Examples:{head:#}\n");
    for (c, what) in rows {
        s += &format!("  {cmd}{c:<36}{cmd:#}{dim}{what}{dim:#}\n");
    }
    s
}

#[derive(Parser)]
#[command(
    name = "atlas",
    version = VERSION.as_str(),
    about = "See how code works: where data rests, and the functions that carry it there.",
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
}

#[derive(Subcommand)]
enum Cmd {
    /// Draw one or more repositories and open the picture in the browser.
    View {
        /// Repository roots or git URLs (a `…/tree/<ref>/<folder>` URL names one folder); default:
        /// the current directory. Several become parts of one page.
        repos: Vec<PathBuf>,
        /// An `architecture.yml` or `roles.json` for a single repo; default: the repo's own, else inferred.
        #[arg(long)]
        roles: Option<PathBuf>,
        /// Where to write the page; default: a temporary file.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Write the page without opening a browser.
        #[arg(long)]
        no_open: bool,
        /// Mark what the working tree changed since this git revision (one repository).
        #[arg(long)]
        since: Option<String>,
    },
    /// List where the code and the design principles disagree, each with a question.
    Check {
        /// Repository root or git URL; default: the current directory.
        repo: Option<PathBuf>,
        /// Print the findings as JSON.
        #[arg(long)]
        json: bool,
    },
    /// What the working tree changes against a git revision: routes, their steps and places, findings.
    Changes {
        /// The revision to compare with; default: HEAD.
        rev: Option<String>,
        /// Repository root; default: the current directory.
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Print the changes as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Serve Atlas to a coding agent over MCP (stdio): principles, check, explain, routes, route, changes.
    Mcp,
    /// Write a repository's graph as JSON.
    Analyze {
        /// Repository root, a package directory, or a git URL.
        repo: PathBuf,
        /// Package directory, when discovery picks the wrong one.
        #[arg(long)]
        package: Option<PathBuf>,
        /// An `architecture.yml` or spike-style `roles.json`; default: the repo's own, else inferred.
        #[arg(long)]
        roles: Option<PathBuf>,
        /// Where to write the graph.
        #[arg(short, long, default_value = "graph.json")]
        out: PathBuf,
    },
}

/// The spike viewer, baked into the binary until the frontend replaces it, with its fonts.
const VIEWER: &str = include_str!("../../spike/viewer.html");
const FONTS: &str = include_str!("../../spike/fonts.css");

fn view(repos: &[PathBuf], roles: Option<&std::path::Path>, out: Option<PathBuf>, open: bool, since: Option<&str>) -> Result<()> {
    let g = parts::analyze_repo(repos, roles)?;
    let parts: Vec<&str> = g.parts.iter().map(|p| p.id.as_str()).collect();
    eprintln!("{} · {} · {} modules · {} entries", g.root, parts.join(" + "), g.stats.modules, g.nodes.iter().filter(|n| n.entry.is_some()).count());
    let mut projects = serde_json::Map::new();
    let mut page = serde_json::to_value(&g)?;
    if let Some(rev) = since {
        let [repo] = repos else { anyhow::bail!("--since compares one repository with its history") };
        page["changes"] = serde_json::to_value(atlas_engine::changes::since(repo, rev)?)?;
    }
    projects.insert(g.root.clone(), page);
    let payload = serde_json::to_string(&serde_json::json!({ "projects": projects, "atlas": VERSION.as_str() }))?.replace("</", "<\\/");
    let html = VIEWER.replacen("/*__FONTS__*/", FONTS, 1).replacen("/*__DATA__*/null", &payload, 1);
    let out = out.unwrap_or_else(|| {
        let dir = std::env::temp_dir().join("atlas");
        let _ = std::fs::create_dir_all(&dir);
        let name = projects.keys().cloned().collect::<Vec<_>>().join("+");
        dir.join(format!("{name}.html"))
    });
    std::fs::write(&out, html)?;
    eprintln!("wrote {}", out.display());
    if open {
        #[cfg(target_os = "macos")]
        let status = std::process::Command::new("open").arg(&out).status();
        #[cfg(target_os = "linux")]
        let status = std::process::Command::new("xdg-open").arg(&out).status();
        #[cfg(target_os = "windows")]
        let status = std::process::Command::new("cmd").args(["/C", "start", ""]).arg(&out).status();
        if let Err(e) = status {
            eprintln!("could not open a browser ({e}); open the file yourself");
        }
    }
    Ok(())
}

/// A repository argument as a folder on disk: a path as it is, a URL cloned for as long as the
/// command runs.
fn local(arg: PathBuf) -> Result<(PathBuf, Option<remote::Clone>)> {
    let spec = arg.to_string_lossy().to_string();
    if !remote::is_url(&spec) {
        return Ok((arg, None));
    }
    eprintln!("cloning {spec}");
    let c = remote::clone(&spec)?;
    Ok((c.dir.clone(), Some(c)))
}

/// The findings, grouped by principle, each ending in its question; exit status 1 when there
/// is anything to ask.
fn check(repo: &std::path::Path, json: bool) -> Result<bool> {
    let mut clean = true;
    // each part in its own language: what a Python package declares, a crate's compiler checks
    for g in parts::analyze_parts(&[repo.to_path_buf()], None)? {
        let findings = principles::check(&g);
        if json {
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "root": g.root, "findings": findings }))?);
        } else {
            print!("{}", principles::report(&g.root, &findings));
        }
        clean &= findings.is_empty();
    }
    Ok(clean)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    atlas_engine::python::read_libraries_to(cli.libraries);
    match cli.cmd {
        Cmd::View { repos, roles, out, no_open, since } => {
            let repos = if repos.is_empty() { vec![std::env::current_dir()?] } else { repos };
            let (repos, _clones): (Vec<PathBuf>, Vec<Option<remote::Clone>>) = repos.into_iter().map(local).collect::<Result<Vec<_>>>()?.into_iter().unzip();
            view(&repos, roles.as_deref(), out, !no_open, since.as_deref())?
        }
        Cmd::Mcp => mcp::serve()?,
        Cmd::Changes { rev, repo, json } => {
            let repo = match repo {
                Some(r) => r,
                None => std::env::current_dir()?,
            };
            let c = atlas_engine::changes::since(&repo, rev.as_deref().unwrap_or("HEAD"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&c)?);
            } else {
                print!("{}", atlas_engine::changes::report(&c));
            }
        }
        Cmd::Check { repo, json } => {
            let (repo, _clone) = local(match repo {
                Some(r) => r,
                None => std::env::current_dir()?,
            })?;
            if !check(&repo, json)? {
                std::process::exit(1);
            }
        }
        Cmd::Analyze { repo, package, roles, out } => {
            let (repo, _clone) = local(repo)?;
            let g = match package {
                Some(p) => atlas_engine::analyze(&repo, Some(&p), roles.as_deref())?,
                None => parts::analyze_repo(&[repo.clone()], roles.as_deref())?,
            };
            std::fs::write(&out, serde_json::to_string(&g)?)?;
            let s = &g.stats;
            eprintln!(
                "{} · {} modules · nodes {:?} · edges {:?}",
                g.root, s.modules, s.nodes, s.edges
            );
            eprintln!(
                "unresolved {} (top: {})",
                s.unresolved_calls_total,
                s.unresolved_calls_top.iter().take(8).map(|(k, v)| format!("{k}×{v}")).collect::<Vec<_>>().join(", ")
            );
            eprintln!(
                "roles {}{:?} · entries {} · make targets {}",
                if g.roles.inferred { "(inferred) " } else { "" },
                g.roles.order,
                g.nodes.iter().filter(|n| n.entry.is_some()).count(),
                g.make.len()
            );
            eprintln!("wrote {}", out.display());
        }
    }
    Ok(())
}
