//! A command line of subcommands, with options that every subcommand takes.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
    /// How far to read
    #[arg(long, global = true, default_value_t = 1)]
    rounds: usize,
    /// Keep nothing
    #[arg(long, global = true)]
    no_cache: bool,
}

#[derive(Subcommand)]
enum Cmd {
    Analyze {
        repo: PathBuf,
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
}

fn analyze(repo: &PathBuf, rounds: usize) {
    println!("{} {rounds}", repo.display());
}

fn main() {
    let cli = Cli::parse();
    match &cli.cmd {
        Cmd::Analyze { repo, .. } => analyze(repo, cli.rounds),
    }
}
