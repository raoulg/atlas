//! The workspace Studio has open: the folder of the code it draws, which Studio reads and writes
//! only inside.

use std::fs::FileType;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    /// The workspace at `folder`, as the file system resolves it.
    pub fn open(folder: &Path) -> Result<Workspace, String> {
        let root = folder.canonicalize().map_err(|error| format!("Cannot open workspace: {error}"))?;
        Ok(Workspace { root })
    }

    /// Whether `folder` can be one: a folder that is there.
    pub fn is(folder: &Path) -> bool {
        folder.is_dir()
    }

    /// Its folder, as the file system resolves it.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `path` as the file system resolves it, where that is inside the workspace; `named` is how
    /// the path is said when it cannot be opened, `refusal` why one outside is refused.
    pub fn inside(&self, path: &Path, named: &str, refusal: &str) -> Result<PathBuf, String> {
        let path = path.canonicalize().map_err(|error| format!("Cannot open {named}: {error}"))?;
        if !path.starts_with(&self.root) {
            return Err(refusal.into());
        }
        Ok(path)
    }

    /// The text of a file of it, when it is no larger than `most` bytes, else why not.
    pub fn text(&self, path: &Path, most: u64, too_large: &str) -> Result<String, String> {
        if self.size(path)? > most {
            return Err(too_large.into());
        }
        std::fs::read_to_string(path).map_err(|error| format!("Cannot read {}: {error}", path.display()))
    }

    /// The bytes of a file of it, when it is no larger than `most` bytes, else why not.
    pub fn bytes(&self, path: &Path, most: u64, too_large: String, named: &str) -> Result<Vec<u8>, String> {
        if self.size(path)? > most {
            return Err(too_large);
        }
        std::fs::read(path).map_err(|error| format!("Cannot read {named}: {error}"))
    }

    fn size(&self, path: &Path) -> Result<u64, String> {
        Ok(std::fs::metadata(path).map_err(|error| error.to_string())?.len())
    }

    /// Write `text` to a file of it.
    pub fn write(&self, path: &Path, text: &str) -> Result<(), String> {
        std::fs::write(path, text).map_err(|error| format!("Cannot save {}: {error}", path.display()))
    }

    /// Make a folder of it, with those it is in.
    pub fn folder(&self, path: &Path) -> Result<(), String> {
        std::fs::create_dir_all(path).map_err(|error| error.to_string())
    }

    /// Whether `path` of it is there.
    pub fn has(&self, path: &Path) -> bool {
        path.exists()
    }

    /// Whether the file at `rel` from its folder is there: its `pyproject.toml`, its `.venv`'s Python.
    pub fn holds(&self, rel: &str) -> bool {
        self.root.join(rel).is_file()
    }

    /// What a folder of it holds, each with what kind of entry it is.
    pub fn entries(&self, folder: &Path) -> Vec<(PathBuf, FileType)> {
        let Ok(entries) = std::fs::read_dir(folder) else { return vec![] };
        entries.flatten().filter_map(|e| Some((e.path(), e.file_type().ok()?))).collect()
    }

    /// A file's size and when it was written.
    pub fn stamp(&self, path: &Path) -> Option<(u64, Option<SystemTime>)> {
        path.metadata().ok().map(|m| (m.len(), m.modified().ok()))
    }
}
