mod mcp;

use std::path::PathBuf;

use anyhow::Result;
use atlas_engine::principles;
use clap::builder::styling::{AnsiColor, Effects, Style, Styles};
use clap::{Parser, Subcommand};

/// The viewer's place colours, in the terminal's own palette.
const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Blue.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Blue.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Yellow.on_default())
    .valid(AnsiColor::Green.on_default())
    .invalid(AnsiColor::Red.on_default().effects(Effects::BOLD))
    .error(AnsiColor::Red.on_default().effects(Effects::BOLD));

/// The logo in a line: a place, a function, a place, a function, a place.
fn banner() -> String {
    let (a, b, c) = (AnsiColor::Cyan.on_default(), AnsiColor::Blue.on_default(), AnsiColor::Yellow.on_default());
    let (dim, bold) = (Style::new().effects(Effects::DIMMED), Style::new().effects(Effects::BOLD));
    format!(
        "{a}■{a:#}{dim}──▶{dim:#}{b}■{b:#}{dim}──▶{dim:#}{c}■{c:#}  {bold}atlas{bold:#} {dim}{}{dim:#}",
        env!("CARGO_PKG_VERSION")
    )
}

fn examples() -> String {
    let (head, cmd, dim) = (STYLES.get_header(), STYLES.get_literal(), Style::new().effects(Effects::DIMMED));
    let rows = [
        ("atlas view", "draw the repository you are in and open it in the browser"),
        ("atlas view ../api ../web", "two projects in one page, with a switcher"),
        ("atlas check", "where the code and the design principles disagree"),
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
    version,
    about = "See how code works: where data rests, and the functions that carry it there.",
    before_help = banner(),
    after_help = examples(),
    styles = STYLES
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Draw one or more repositories and open the picture in the browser.
    View {
        /// Repository roots; default: the current directory. Several become a project switcher.
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
    },
    /// List where the code and the design principles disagree, each with a question.
    Check {
        /// Repository root; default: the current directory.
        repo: Option<PathBuf>,
        /// Print the findings as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Serve Atlas to a coding agent over MCP (stdio): principles, check, explain, routes, route.
    Mcp,
    /// Write a repository's graph as JSON.
    Analyze {
        /// Repository root (or a package directory).
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

/// The spike viewer, baked into the binary until the frontend replaces it.
const VIEWER: &str = include_str!("../../spike/viewer.html");

fn view(repos: &[PathBuf], roles: Option<&std::path::Path>, out: Option<PathBuf>, open: bool) -> Result<()> {
    let mut projects = serde_json::Map::new();
    for repo in repos {
        let g = atlas_engine::analyze(repo, None, if repos.len() == 1 { roles } else { None })?;
        eprintln!("{} · {} modules · {} entries", g.root, g.stats.modules, g.nodes.iter().filter(|n| n.entry.is_some()).count());
        projects.insert(g.root.clone(), serde_json::to_value(&g)?);
    }
    let payload = serde_json::to_string(&serde_json::json!({ "projects": projects }))?.replace("</", "<\\/");
    let html = VIEWER.replacen("/*__DATA__*/null", &payload, 1);
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

/// The findings, grouped by principle, each ending in its question; exit status 1 when there
/// is anything to ask.
fn check(repo: &std::path::Path, json: bool) -> Result<bool> {
    let g = atlas_engine::analyze(repo, None, None)?;
    let findings = principles::check(&g);
    if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "root": g.root, "findings": findings }))?);
    } else {
        print!("{}", principles::report(&g.root, &findings));
    }
    Ok(findings.is_empty())
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::View { repos, roles, out, no_open } => {
            let repos = if repos.is_empty() { vec![std::env::current_dir()?] } else { repos };
            view(&repos, roles.as_deref(), out, !no_open)?
        }
        Cmd::Mcp => mcp::serve()?,
        Cmd::Check { repo, json } => {
            let repo = match repo {
                Some(r) => r,
                None => std::env::current_dir()?,
            };
            if !check(&repo, json)? {
                std::process::exit(1);
            }
        }
        Cmd::Analyze { repo, package, roles, out } => {
            let g = atlas_engine::analyze(&repo, package.as_deref(), roles.as_deref())?;
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
