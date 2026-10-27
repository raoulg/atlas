//! Where `atlas view` and `analyze` put what they make, and a served page and an agent share files:
//! the subject's workbench, `.atlas/` (`places::Workbench`).

use std::path::{Path, PathBuf};

use atlas_engine::places::Workbench;

/// What Atlas makes for the subject `subject` when no place is asked for: in the workbench of the
/// worktree it is in, where whoever works there, a person or an agent, finds it. A clone is gone
/// when the command ends, so what is made of one goes to the temporary workbench.
pub(crate) fn made(subject: Option<&Path>, name: &str) -> PathBuf {
    match subject {
        Some(subject) => Workbench::of(subject).file(name),
        None => Workbench::temporary().file(name),
    }
}
