//! Where a project's libraries are kept, which the readers ask for a library's files.

use std::path::{Path, PathBuf};

/// Where a project's libraries are kept: the folders its language's tool fills with the code of
/// what the project depends on (Cargo's registry sources, Node's `node_modules`, Python's
/// site-packages), and what a reading takes from them, as checks of how each stands.
#[derive(Clone, Debug, Default)]
pub struct Environment {
    pub roots: Vec<PathBuf>,
    pub checks: Vec<crate::cache::Check>,
}

impl Environment {
    /// The environment whose libraries lie in `roots`, in the order its tool looks.
    pub fn new(roots: Vec<PathBuf>) -> Environment {
        Environment { roots, checks: vec![] }
    }

    /// The folder of the library `name` (`serde-1.0.200`, `react`): below the first root that
    /// holds it with its `manifest`. Each place looked in is noted as asked.
    pub fn package(&mut self, name: &str, manifest: &str) -> Option<PathBuf> {
        let folders: Vec<PathBuf> = self.roots.iter().map(|root| root.join(name)).collect();
        self.checks.extend(folders.iter().map(|folder| crate::cache::Check::File(folder.join(manifest))));
        folders.into_iter().find(|folder| folder.join(manifest).is_file())
    }

    /// A library's manifest, as its tool writes it (`Cargo.toml`, `package.json`).
    pub fn manifest(&self, package_dir: &Path, manifest: &str) -> Option<String> {
        std::fs::read_to_string(package_dir.join(manifest)).ok()
    }

    /// The folder of the library a file of it lies in: the nearest above it that holds `manifest`.
    pub fn manifest_dir(&self, file: &Path, manifest: &str) -> Option<PathBuf> {
        file.ancestors().find(|folder| folder.join(manifest).is_file()).map(Path::to_path_buf)
    }

    /// The first of `candidates` that is a file of a library: where a module the code declares is.
    pub fn first_file(&self, candidates: Vec<PathBuf>) -> Option<PathBuf> {
        candidates.into_iter().find(|c| c.is_file())
    }

    /// Where a path of a library really lies, as the file system resolves it: a package linked into
    /// `node_modules` lies where the link points.
    pub fn real(&self, path: &Path) -> Option<PathBuf> {
        std::fs::canonicalize(path).ok()
    }

    /// Whether the folder of a library holds compiled code its `registry` ships (`.so`, `.node`),
    /// which Atlas does not read as code.
    pub fn compiled(&self, folder: &Path, registry: crate::knowledge::Registry) -> bool {
        let compiled = crate::knowledge::compiled(registry);
        walkdir::WalkDir::new(folder)
            .into_iter()
            .flatten()
            .any(|e| e.path().extension().and_then(|x| x.to_str()).map_or(false, |x| compiled.contains(&x)))
    }

    /// A library's source file, as it is.
    pub fn source(&self, file: &Path) -> Option<Vec<u8>> {
        std::fs::read(file).ok()
    }
}
