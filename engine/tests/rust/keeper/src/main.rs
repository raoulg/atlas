//! A program that writes its graph where `-o` says, else where `made` puts it: in `.made/` at
//! the top of the git repository, or for a clone in the system's temporary folder.

use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;

#[derive(Parser)]
struct Cli {
    repo: PathBuf,
    #[arg(short, long)]
    out: Option<PathBuf>,
    #[arg(long)]
    clone: bool,
}

/// Where generated files for `dir` are kept, at its git repository's top when it has one.
fn home(dir: &Path) -> PathBuf {
    let top = dir.ancestors().find(|a| a.join(".git").exists()).unwrap_or(dir);
    let home = top.join(".made");
    let _ = fs::create_dir_all(&home);
    home
}

/// Where a made `file` goes: in the home of `dir`, else in the temporary folder.
fn made(dir: Option<&Path>, file: &str) -> PathBuf {
    let Some(dir) = dir else {
        let tmp = std::env::temp_dir().join("made");
        let _ = fs::create_dir_all(&tmp);
        return tmp.join(file);
    };
    home(dir).join(file)
}

fn main() -> std::io::Result<()> {
    let Cli { repo, out, clone } = Cli::parse();
    let out = out.unwrap_or_else(|| made((!clone).then_some(repo.as_path()), "graph.json"));
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(&out, "{}")?;
    Ok(())
}
