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
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
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
