use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "atlas", version, about = "Atlas: code at the level of types and flows")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Parse a repository into its graph (JSON).
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
    /// Analyze one or more repositories and open them in the viewer.
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

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::View { repos, roles, out, no_open } => {
            let repos = if repos.is_empty() { vec![std::env::current_dir()?] } else { repos };
            view(&repos, roles.as_deref(), out, !no_open)?
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
