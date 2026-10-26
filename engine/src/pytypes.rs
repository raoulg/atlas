//! Python's types, as ty sees them: where a call goes, and what a value is.
//!
//! Atlas reads a Python project's structure with tree-sitter and asks ty, Astral's type
//! checker, for its types. ty reads the project with the project's own environment: the one its
//! `[tool.ty.environment]` names, else its `.venv`, never what is active in the shell Atlas runs
//! in. The two trees meet at an expression's byte range in its file.
//!
//! Where ty says a type is unknown, so does Atlas: a function without a return annotation
//! gives back `Unknown`, and nothing is guessed behind it.
//!
//! What ty says of a file is first written down as where each answer lies (a file and an offset,
//! and the module ty resolves the file to), and named once it is known which files Atlas reads:
//! one of them by its name there, any other as a library's module. What ty says of an installed
//! library's file is the same for every project read in the same environment, and is kept
//! (`cache`), by the file's bytes and by everything ty finds its imports in.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use ruff_db::diagnostic::{Diagnostic, Severity, UnifiedFile};
use ruff_db::files::{system_path_to_file, File, FilePath, FileRange, FileRootKind, Files};
use ruff_db::parsed::parsed_module;
use ruff_db::system::{OsSystem, System, SystemPath, SystemPathBuf};
use ruff_db::vendored::VendoredFileSystem;
use ruff_db::Db as SourceDb;
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast, AnyNodeRef, PythonVersion};
use ruff_text_size::{Ranged, TextRange};
use ty_module_resolver::{file_to_module, Db as ModuleResolverDb, FallibleStrategy, SearchPathSettings, SearchPaths};
use ty_python_core::platform::PythonPlatform;
use ty_python_core::program::{Program, ProgramSettings};
use ty_python_core::{Db as PythonCoreDb, ProgramFile};
use ty_python_semantic::dependency::DependencyMetadata;
use ty_python_semantic::lint::{LintRegistry, LintSource, RuleSelection};
use ty_python_semantic::types::{Type, TypeDefinition, TypeDetail};
use ty_python_semantic::{
    check_file_unwrap, default_lint_registry, definitions_for_attribute, definitions_for_name, AnalysisSettings, Db, HasType,
    ImportAliasResolution, PythonEnvironment, PythonVersionSource, PythonVersionWithSource, SemanticModel, SysPrefixPathOrigin,
};

use crate::cache::{Cache, Check, Imports};

/// Where a definition lives: in the project, at its name, or in a library's module.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// A file Atlas reads, and the byte offset of the definition's name.
    Here { file: String, at: usize },
    /// The module a library defines it in: `neo4j._sync.work.session`, `pathlib`, `builtins`.
    Lib(String),
}

/// How a call reaches its target.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum By {
    /// A function or a method is called.
    Call,
    /// A class is called: a value is made.
    Construct,
    /// An instance is called: its class's `__call__`.
    Instance,
    /// What the called name is defined as, when its type is neither of the above (a decorated
    /// function, a union of methods).
    Name,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Callee {
    pub target: Target,
    pub by: By,
}

/// What a parameter of a library function is handed, as its type says: a path, a file to read
/// from (`SupportsRead`), one to write to (`SupportsWrite`), an open file.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum FileArg {
    Path,
    Reads,
    Writes,
    Open,
}

/// A parameter of a library function: its name, and what it is handed when that is a file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub file: Option<FileArg>,
}

/// The standard library's types for a path and for a file, by the class that defines them.
fn file_of(class: &str) -> Option<FileArg> {
    match class {
        "PathLike" | "PurePath" | "Path" | "PurePosixPath" | "PureWindowsPath" | "PosixPath" | "WindowsPath" => Some(FileArg::Path),
        "SupportsRead" | "SupportsReadline" => Some(FileArg::Reads),
        "SupportsWrite" => Some(FileArg::Writes),
        "IO" | "TextIO" | "BinaryIO" | "IOBase" | "RawIOBase" | "BufferedIOBase" | "TextIOBase" | "TextIOWrapper" | "BufferedReader"
        | "BufferedWriter" | "BufferedRandom" | "FileIO" => Some(FileArg::Open),
        _ => None,
    }
}

/// A value's type: as ty writes it, the classes it names (`list[Node]` names `list` and
/// `Node`) with each class's own name, whether the value is a class itself rather than an
/// instance, and whether it is code (a function, a method, a module) rather than data.
#[derive(Clone, Debug, PartialEq)]
pub struct Value {
    pub shown: String,
    pub classes: Vec<Target>,
    pub names: Vec<String>,
    pub is_class: bool,
    pub is_code: bool,
}

/// The classes a type is, as written: through the containers that hold values (`tuple`,
/// `list`, `dict`, a union) to each value's own class, but not into a class's own parameters
/// (`ndarray[tuple[int], dtype[float64]]` is an `ndarray`), nor into a pair a collection holds,
/// which is of a type the code does not name (`list[tuple[date, float]]` holds no `date`), nor into
/// a map's keys (`dict[Mode, Plan]` holds Plans by their Mode).
pub fn outer_classes(label: &str) -> Vec<String> {
    classes_within(label, false)
}

/// What follows the first parameter of a generic's parameters: `Plan` of `Mode, Plan`.
fn past_first(params: &str) -> &str {
    let mut depth = 0;
    for (i, c) in params.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth == 0 => return &params[i + 1..],
            _ => {}
        }
    }
    ""
}

fn classes_within(label: &str, in_collection: bool) -> Vec<String> {
    // split at depth 0 on `,` and `|`
    fn parts(s: &str) -> Vec<&str> {
        let (mut out, mut depth, mut start) = (vec![], 0, 0);
        for (i, c) in s.char_indices() {
            match c {
                '[' => depth += 1,
                ']' => depth -= 1,
                ',' | '|' if depth == 0 => {
                    out.push(s[start..i].trim());
                    start = i + 1;
                }
                _ => {}
            }
        }
        out.push(s[start..].trim());
        out
    }
    let mut out = vec![];
    for p in parts(label) {
        let (head, inner) = match p.find('[') {
            Some(i) if p.ends_with(']') => (&p[..i], Some(&p[i + 1..p.len() - 1])),
            _ => (p, None),
        };
        let pair = crate::knowledge::PYTHON_TUPLES.contains(&head) && inner.is_some_and(|i| !i.trim_end().ends_with("..."));
        match inner {
            Some(_) if pair && in_collection => {}
            Some(inner) if crate::knowledge::PYTHON_ALTERNATIVES.contains(&head) => out.extend(classes_within(inner, in_collection)),
            Some(inner) if crate::knowledge::PYTHON_HOLDERS.contains(&head) => {
                // a map holds its values by its keys, its first parameter: a key is no value it holds
                let inner = if crate::knowledge::PYTHON_MAPS.contains(&head) { past_first(inner) } else { inner };
                out.extend(classes_within(inner, crate::knowledge::python_collection(head) || (in_collection && !pair)))
            }
            _ if !head.is_empty() && head != "..." => out.push(head.to_string()),
            _ => {}
        }
    }
    out
}

/// What ty says about one file, by byte range.
#[derive(Default, Debug)]
pub struct Facts {
    callees: HashMap<(usize, usize), Vec<Callee>>,
    values: HashMap<(usize, usize), Value>,
    defs: HashMap<(usize, usize), Target>,
    signatures: HashMap<(usize, usize), Vec<Param>>,
    keeps: HashMap<usize, bool>,
    breaks: HashMap<usize, Target>,
}

impl Facts {
    /// Where a call to the expression at `range` goes.
    pub fn callees(&self, range: (usize, usize)) -> &[Callee] {
        self.callees.get(&range).map_or(&[], Vec::as_slice)
    }

    /// The type of the expression at `range`, when ty knows it.
    pub fn value(&self, range: (usize, usize)) -> Option<&Value> {
        self.values.get(&range)
    }

    /// The parameters of the library function the expression at `range` calls.
    pub fn signature(&self, range: (usize, usize)) -> Option<&[Param]> {
        self.signatures.get(&range).map(Vec::as_slice)
    }

    /// Where the name or attribute at `range` is defined, as a reader's goto-definition finds it.
    pub fn def(&self, range: (usize, usize)) -> Option<&Target> {
        self.defs.get(&range)
    }

    /// For the method whose name is written at `at`, when it overrides one of a class of the
    /// project's own: whether it keeps the promise of each method it overrides, as ty's check of an
    /// override decides it.
    pub fn keeps(&self, at: usize) -> Option<bool> {
        self.keeps.get(&at).copied()
    }

    /// For the method whose name is written at `at`, when it does not keep its promise: the method
    /// whose promise ty finds it breaks, where a file Atlas reads defines it.
    pub fn breaks(&self, at: usize) -> Option<&Target> {
        self.breaks.get(&at)
    }
}

/// ty's answers for a project's files.
#[derive(Default, Debug)]
pub struct Types {
    files: HashMap<String, Facts>,
    /// Whether an environment with libraries was found.
    pub environment: bool,
    /// The library modules the files' calls go into: each module's name and its file.
    pub libs: Vec<(String, PathBuf)>,
}

impl Types {
    /// The facts for one file, by the path Atlas knows it by.
    pub fn of(&self, file: &str) -> Option<&Facts> {
        self.files.get(file)
    }

    /// Ask ty about `files`, read as part of the project at `root`. A file is named by the path
    /// Atlas knows it by; a notebook's positions are in its cells joined as ruff joins them.
    pub fn read(root: &Path, files: &[(String, PathBuf)]) -> Types {
        Ty::new(root, files, None).map(|ty| ty.read(files, files)).unwrap_or_default()
    }
}

/// Where a definition lies as ty finds it, before it is known which files Atlas reads: its file
/// on disk (by its key, and by ty's path for it), the byte offset of its name, and the module ty
/// resolves its file to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Place {
    key: Option<String>,
    path: Option<PathBuf>,
    at: usize,
    module: Option<String>,
}

/// What a call reaches as ty sees it: by the type of what is called, and where that may lead
/// nowhere Atlas names, by where the called name is defined and by the class of the instance.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Reached {
    typed: Option<(Place, By)>,
    named: Vec<Place>,
    instance: Option<Place>,
}

impl Reached {
    fn places(&self) -> impl Iterator<Item = &Place> {
        self.typed.iter().map(|(p, _)| p).chain(&self.named).chain(&self.instance)
    }
}

/// A value's type as ty writes it, with the classes of the values it holds and where each is
/// defined.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Shown {
    shown: String,
    classes: Vec<(Place, String)>,
    is_class: bool,
    is_code: bool,
}

/// What ty says of one file, in the order it says it, before it is known which files Atlas reads.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Said {
    /// What each call reaches, with the parameters of the library function it may call, and what
    /// each function handed to a call reaches.
    calls: Vec<((usize, usize), Reached, Option<Vec<Param>>)>,
    values: Vec<((usize, usize), Shown)>,
    defs: Vec<((usize, usize), Place)>,
}

impl Said {
    fn places(&self) -> impl Iterator<Item = &Place> {
        let calls = self.calls.iter().flat_map(|(_, r, _)| r.places());
        calls.chain(self.values.iter().flat_map(|(_, v)| v.classes.iter().map(|(p, _)| p))).chain(self.defs.iter().map(|(_, p)| p))
    }
}

/// The files Atlas reads, by their keys, with the name Atlas knows each by: what ty said is named
/// by them. The library files calls go into are noted with their modules' names.
struct Here<'a> {
    names: &'a HashMap<String, String>,
    libs: RefCell<BTreeMap<PathBuf, String>>,
}

impl Here<'_> {
    /// Where a place lies: a file Atlas reads, or a library's module.
    fn target(&self, p: &Place) -> Option<Target> {
        match p.key.as_ref().and_then(|k| self.names.get(k)) {
            Some(name) => Some(Target::Here { file: name.clone(), at: p.at }),
            None => p.module.clone().map(Target::Lib),
        }
    }

    /// Where a call goes, noting the library file it goes into.
    fn called(&self, p: &Place) -> Option<Target> {
        let target = self.target(p)?;
        if let (Target::Lib(module), Some(path)) = (&target, &p.path) {
            self.libs.borrow_mut().insert(path.clone(), module.clone());
        }
        Some(target)
    }

    /// Where calling goes: by the type of what is called when that is a function or a class,
    /// else by where its name is defined, and for an instance also by its class.
    fn callees(&self, r: &Reached) -> Vec<Callee> {
        if let Some((p, by)) = &r.typed {
            if let Some(target) = self.called(p) {
                return vec![Callee { target, by: *by }];
            }
        }
        let mut out: Vec<Callee> = r.named.iter().filter_map(|p| self.called(p)).map(|target| Callee { target, by: By::Name }).collect();
        out.extend(r.instance.as_ref().and_then(|p| self.called(p)).map(|target| Callee { target, by: By::Instance }));
        out.dedup();
        out
    }

    /// What ty said of a file, named. A library function's parameters are kept where a call
    /// goes into a library's module.
    fn facts(&self, said: &Said) -> Facts {
        let mut f = Facts::default();
        for (range, reached, signature) in &said.calls {
            let callees = self.callees(reached);
            if let Some(sig) = signature.as_ref().filter(|_| callees.iter().any(|c| matches!(c.target, Target::Lib(_)))) {
                f.signatures.insert(*range, sig.clone());
            }
            if !callees.is_empty() {
                f.callees.insert(*range, callees);
            }
        }
        for (range, v) in &said.values {
            let (classes, names) = v.classes.iter().filter_map(|(p, n)| Some((self.target(p)?, n.clone()))).unzip();
            f.values.insert(*range, Value { shown: v.shown.clone(), classes, names, is_class: v.is_class, is_code: v.is_code });
        }
        for (range, p) in &said.defs {
            if let Some(t) = self.target(p) {
                f.defs.insert(*range, t);
            }
        }
        f
    }
}

/// ty over one project, asked more than once: what it has said of a file is kept from one
/// question to the next, and what it says of an installed library's file from one reading to
/// the next.
pub struct Ty {
    db: TyDb,
    environment: bool,
    /// The project's own files, by their keys.
    own: HashSet<String>,
    said: RefCell<HashMap<String, Arc<Said>>>,
    keys: RefCell<HashMap<PathBuf, String>>,
    kept: Option<Kept>,
}

/// Where what ty says of an installed library's file is kept: under everything ty finds the
/// library's imports in, as it stands, and only for a file of the environment's own
/// distributions whose answers lie there too.
struct Kept {
    cache: Cache,
    setup: Setup,
    /// The setup's fingerprint, taken when a library's file is first asked about.
    environment: std::cell::OnceCell<String>,
    /// The site-packages directories, by their keys.
    installed: Vec<String>,
}

impl Kept {
    fn new(cache: &Cache, setup: Setup) -> Kept {
        let installed = setup.installed().iter().map(|d| key(d)).collect();
        Kept { cache: cache.clone(), setup, environment: std::cell::OnceCell::new(), installed }
    }

    /// Whether the file at `key` is an installed distribution's.
    fn holds(&self, key: &str) -> bool {
        self.installed.iter().any(|d| key.strip_prefix(d.as_str()).map_or(false, |rest| rest.starts_with('/')))
    }

    /// The key of what ty says of the file at `key`, by its bytes; none for a file no installed
    /// distribution holds.
    fn key(&self, key: &str) -> Option<String> {
        if !self.holds(key) {
            return None;
        }
        let bytes = std::fs::read(key).ok()?;
        let environment = self.environment.get_or_init(|| self.setup.fingerprint());
        Some(format!("{environment}\n{key}\n{}", crate::cache::digest(&bytes)))
    }

    fn get(&self, key: &str) -> Option<Said> {
        serde_json::from_slice(&self.cache.get(KEPT, key)?).ok()
    }

    /// Keep what ty said of a file, unless an answer lies outside the environment's distributions
    /// and the standard library, or in a file ty names otherwise than the file system resolves
    /// it: it would then depend on more than the key holds.
    fn put(&self, key: &str, said: &Said) {
        let inside =
            |p: &Place| p.key.as_ref().map_or(true, |k| self.holds(k) && p.path.as_deref().map_or(true, |path| keyed_as(path) == *k));
        if said.places().all(inside) {
            if let Ok(bytes) = serde_json::to_vec(said) {
                self.cache.put(KEPT, key, &bytes);
            }
        }
    }
}

/// The kind of entry what ty says of a library's file is kept as.
const KEPT: &str = "python-types";

impl Ty {
    /// ty over the project at `root`, whose own files are `own`; what it says of an installed
    /// library's file is kept in `cache`.
    pub fn new(root: &Path, own: &[(String, PathBuf)], cache: Option<&Cache>) -> Option<Ty> {
        let root = absolute(root)?;
        let (db, setup) = TyDb::new(&root);
        let environment = setup.environment;
        let kept = cache.map(|c| Kept::new(c, setup));
        let own = own.iter().filter_map(|(_, p)| absolute(p)).map(|p| key(p.as_std_path())).collect();
        Some(Ty { db, environment, own, said: RefCell::default(), keys: RefCell::default(), kept })
    }

    /// Ask about `files`, with `project` the files Atlas reads: a call into one of them goes to
    /// its definition there, a call elsewhere into a library's module.
    pub fn read(&self, files: &[(String, PathBuf)], project: &[(String, PathBuf)]) -> Types {
        let names = keyed(project);
        let here = Here { names: &names, libs: RefCell::default() };
        let mut out = Types { files: HashMap::new(), environment: self.environment, libs: vec![] };
        let mut classes = vec![];
        for (path, name) in asked(files) {
            let Some(said) = self.said(&path) else { continue };
            out.files.insert(name.clone(), here.facts(&said));
            // a library read as code is not the project's to change: what it overrides is not asked
            if self.own.contains(&key(path.as_std_path())) {
                classes.extend(self.classes(&path, &name));
            }
        }
        let mut broken: HashMap<String, Vec<Broken>> = HashMap::new();
        for o in overrides(&classes, &names) {
            let path = classes.iter().find(|c| c.file == o.file).map(|c| &c.path);
            let found = broken.entry(o.file.clone()).or_insert_with(|| path.map_or_else(Vec::new, |p| self.broken(p)));
            let within = |r: &TextRange, at: usize| r.start().to_usize() <= at && at < r.end().to_usize();
            let here = found.iter().find(|b| within(&b.signature, o.at));
            // the method ty names as the one whose promise it breaks: the base whose signature it marks
            let base = here.and_then(|b| o.bases.iter().find(|(key, at, _)| b.bases.iter().any(|(k, r)| k == key && within(r, *at))));
            if let Some(f) = out.files.get_mut(&o.file) {
                f.keeps.insert(o.at, here.is_none());
                if let Some((_, at, file)) = base {
                    f.breaks.insert(o.at, Target::Here { file: file.clone(), at: *at });
                }
            }
        }
        out.libs = here.libs.into_inner().into_iter().map(|(path, module)| (module, path)).collect();
        out
    }

    /// The library modules the calls of `files` go into, each with its file, with `project` the
    /// files Atlas reads.
    pub fn libs(&self, files: &[(String, PathBuf)], project: &[(String, PathBuf)]) -> Vec<(String, PathBuf)> {
        let names = keyed(project);
        let here = Here { names: &names, libs: RefCell::default() };
        for (path, _) in asked(files) {
            if let Some(said) = self.said(&path) {
                here.facts(&said);
            }
        }
        here.libs.into_inner().into_iter().map(|(path, module)| (module, path)).collect()
    }

    /// What ty says of the file at `path`: asked once, or for an installed library's file kept
    /// from a reading before.
    fn said(&self, path: &SystemPathBuf) -> Option<Arc<Said>> {
        let k = key(path.as_std_path());
        if let Some(s) = self.said.borrow().get(&k) {
            return Some(s.clone());
        }
        let file = system_path_to_file(&self.db, path).ok()?;
        let own = self.own.contains(&k);
        let kept = self.kept.as_ref().filter(|_| !own).and_then(|kept| Some((kept, kept.key(&k)?)));
        let said = match kept.as_ref().and_then(|(kept, entry)| kept.get(entry)) {
            Some(s) => s,
            None => {
                // a call into a file of the project's own is named there whichever files are read;
                // of a library's file only its own calls are, so what it says holds for any project
                let alone = HashSet::from([k.clone()]);
                let s = self.ask(file, if own { &self.own } else { &alone });
                if let Some((kept, entry)) = &kept {
                    kept.put(entry, &s);
                }
                s
            }
        };
        let s = Arc::new(said);
        self.said.borrow_mut().insert(k, s.clone());
        Some(s)
    }

    /// Walk a file and write down what ty says of it, with `certain` the files a call into which
    /// is surely named where it goes.
    fn ask(&self, file: File, certain: &HashSet<String>) -> Said {
        let db = &self.db;
        let pf = db.program_file(file);
        let model = SemanticModel::new(db, pf);
        let parsed = parsed_module(db, pf.python_file(db)).load(db);
        let mut reader = Reader { db, model: &model, certain, keys: &self.keys, said: Said::default() };
        reader.visit_body(parsed.suite());
        reader.said
    }

    /// The classes of the file at `path`, known by `name`, for what their methods override.
    fn classes(&self, path: &SystemPathBuf, name: &str) -> Vec<Class> {
        let db = &self.db;
        let Ok(file) = system_path_to_file(db, path) else { return vec![] };
        let pf = db.program_file(file);
        let model = SemanticModel::new(db, pf);
        let parsed = parsed_module(db, pf.python_file(db)).load(db);
        let none = HashSet::new();
        let reader = Reader { db, model: &model, certain: &none, keys: &self.keys, said: Said::default() };
        let mut found = Classes { reader, file: name, path, key: key(path.as_std_path()), out: vec![] };
        found.visit_body(parsed.suite());
        found.out
    }

    /// Where ty's check of the file at `path` finds a method that does not keep the promise of one
    /// it overrides. A `# type: ignore[...]` on the first or last line of its signature is the code
    /// saying it knows, whatever its codes, as the bare `# type: ignore` is: ty itself honours only
    /// the bare form and codes of its own (`ty:`).
    fn broken(&self, path: &SystemPathBuf) -> Vec<Broken> {
        let db = &self.db;
        let Ok(file) = system_path_to_file(db, path) else { return vec![] };
        let found: Vec<Diagnostic> = db.check_file(file).into_iter().filter(|d| d.id().is_lint_named(OVERRIDES)).collect();
        if found.is_empty() {
            return vec![];
        }
        let text = ruff_db::source::source_text(db, file);
        let parsed = parsed_module(db, db.program_file(file).python_file(db)).load(db);
        let ignores: Vec<usize> = parsed
            .tokens()
            .iter()
            .filter(|t| t.kind() == TokenKind::Comment && ignores_with_codes(&text[t.range()]))
            .map(|t| t.start().to_usize())
            .collect();
        let line = |at: usize| {
            let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
            (start, text[at..].find('\n').map_or(text.len(), |i| at + i))
        };
        let ignored = |r: TextRange| {
            [r.start().to_usize(), r.end().to_usize()].into_iter().map(line).any(|(a, b)| ignores.iter().any(|c| a <= *c && *c <= b))
        };
        found
            .iter()
            .filter_map(|d| {
                let signature = d.range().filter(|r| !ignored(*r))?;
                let bases = d
                    .secondary_annotations()
                    .filter_map(|a| {
                        let UnifiedFile::Ty(f) = a.get_span().file() else { return None };
                        Some((key(f.path(db).as_system_path()?.as_std_path()), a.get_span().range()?))
                    })
                    .collect();
                Some(Broken { signature, bases })
            })
            .collect()
    }
}

/// A method ty finds breaking the promise of one it overrides: its signature, and the signatures
/// ty marks beside it, each by the key of its file; the method whose promise it breaks is among them.
struct Broken {
    signature: TextRange,
    bases: Vec<(String, TextRange)>,
}

/// Whether a comment holds a `type: ignore` with codes (`# type: ignore[override]`), alone or after
/// another comment on its line (`# noqa # type: ignore[override]`).
fn ignores_with_codes(comment: &str) -> bool {
    comment.split('#').any(|c| {
        c.trim().strip_prefix("type:").and_then(|c| c.trim_start().strip_prefix("ignore")).is_some_and(|c| c.trim_start().starts_with('['))
    })
}

/// The files of a list ty reads, by their paths on disk, with the names Atlas knows them by.
fn asked(list: &[(String, PathBuf)]) -> Vec<(SystemPathBuf, String)> {
    list.iter()
        .filter_map(|(name, path)| absolute(path).filter(|p| matches!(p.extension(), Some("py" | "ipynb"))).map(|p| (p, name.clone())))
        .collect()
}

/// The files of a list by their keys, with the names Atlas knows them by.
fn keyed(list: &[(String, PathBuf)]) -> HashMap<String, String> {
    asked(list).into_iter().map(|(p, name)| (key(p.as_std_path()), name)).collect()
}

/// The check of ty's that Atlas asks for: whether a method keeps the promise of each one it
/// overrides, along its class's method resolution order, and if not, the first whose promise it
/// breaks. It reads the whole promise, what the method takes and what it gives, and reads a
/// generic base as the subclass sees it: in `class Boekwinkel(Winkel[Boek])`, a `-> Versie[R]`
/// of `Winkel[R]` promises `Versie[Boek]`.
const OVERRIDES: &str = "invalid-method-override";

/// The checks ty runs: only the one Atlas asks for.
fn checks() -> RuleSelection {
    let mut rules = RuleSelection::default();
    if let Ok(lint) = default_lint_registry().get(OVERRIDES) {
        rules.enable(lint, Severity::Error, LintSource::Default);
    }
    rules
}

/// A class as ty sees it, for what its methods override: its file, where each of its bases is
/// defined, and each method by its name and where that name is written.
struct Class {
    file: String,
    path: SystemPathBuf,
    key: String,
    at: usize,
    bases: Vec<Place>,
    methods: Vec<(String, usize)>,
}

/// A method that overrides one of a class read with it: its file, where its name is written, and
/// each method of its class's ancestors it overrides, by the key of its file, where its name is
/// written and the name Atlas knows the file by. ty holds it to every one of them.
struct Override {
    file: String,
    at: usize,
    bases: Vec<(String, usize, String)>,
}

/// Each method that overrides one of a class read with it.
fn overrides(classes: &[Class], here: &HashMap<String, String>) -> Vec<Override> {
    let by_place: HashMap<(&str, usize), &Class> = classes.iter().map(|c| ((c.key.as_str(), c.at), c)).collect();
    // the bases a class is read with: those defined in a file Atlas reads
    let bases = |c: &Class| -> Vec<(String, usize)> {
        c.bases.iter().filter_map(|p| Some((p.key.clone().filter(|k| here.contains_key(k))?, p.at))).collect()
    };
    let mut out = vec![];
    for c in classes {
        for (name, at) in &c.methods {
            let mut seen = HashSet::new();
            let mut queue: VecDeque<(String, usize)> = bases(c).into();
            let mut overridden = vec![];
            while let Some(b) = queue.pop_front() {
                let Some(base) = by_place.get(&(b.0.as_str(), b.1)).copied() else { continue };
                if !seen.insert(b) {
                    continue;
                }
                for (_, m) in base.methods.iter().filter(|(n, _)| n == name) {
                    overridden.push((base.key.clone(), *m, base.file.clone()));
                }
                queue.extend(bases(base));
            }
            if !overridden.is_empty() {
                out.push(Override { file: c.file.clone(), at: *at, bases: overridden });
            }
        }
    }
    out
}

/// A file as one key whatever way its path is written: resolved, and on Windows without the
/// verbatim prefix (`\\?\`) and in one case, as the file system compares it.
fn key(p: &Path) -> String {
    keyed_as(&std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()))
}

/// A path as a key, as it is written: the key of a path the file system resolves to itself.
fn keyed_as(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let s = s.strip_prefix("//?/").unwrap_or(&s).to_string();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

fn absolute(p: &Path) -> Option<SystemPathBuf> {
    SystemPathBuf::from_path_buf(std::fs::canonicalize(p).ok()?).ok()
}

/// The parts of a project's ty configuration that change what ty sees: `[tool.ty.environment]`
/// in `pyproject.toml`, or `[environment]` in `ty.toml`.
#[derive(Default)]
struct Settings {
    python: Option<String>,
    python_version: Option<String>,
    extra_paths: Vec<String>,
    roots: Vec<String>,
    file: Option<SystemPathBuf>,
    /// The project's name and the lowest Python it supports, from `[project]`.
    name: Option<String>,
    requires: Option<String>,
}

impl Settings {
    fn of(root: &SystemPath) -> Settings {
        let read = |name: &str| std::fs::read_to_string(root.join(name).as_std_path()).ok().and_then(|s| s.parse::<toml::Table>().ok());
        let (table, file) = match read("ty.toml") {
            Some(t) => (t.get("environment").cloned(), root.join("ty.toml")),
            None => {
                (read("pyproject.toml").and_then(|t| t.get("tool")?.get("ty")?.get("environment").cloned()), root.join("pyproject.toml"))
            }
        };
        let project = read("pyproject.toml").and_then(|t| t.get("project").cloned());
        let name = project.as_ref().and_then(|p| p.get("name")?.as_str().map(String::from));
        let requires = project.as_ref().and_then(|p| p.get("requires-python")?.as_str().map(lowest_version)).flatten();
        let Some(env) = table else { return Settings { name, requires, ..Settings::default() } };
        let strings = |key: &str| -> Vec<String> {
            match env.get(key) {
                Some(toml::Value::Array(xs)) => xs.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
                Some(toml::Value::String(s)) => vec![s.clone()],
                _ => vec![],
            }
        };
        Settings {
            python: env.get("python").and_then(|v| v.as_str()).map(String::from),
            python_version: env.get("python-version").and_then(|v| v.as_str()).map(String::from),
            extra_paths: strings("extra-paths"),
            roots: strings("root"),
            file: Some(file),
            name,
            requires,
        }
    }
}

/// The lowest version a `requires-python` allows: `>=3.11` and `~=3.11.2` are `3.11`.
fn lowest_version(spec: &str) -> Option<String> {
    let lower = spec.split(',').map(str::trim).find(|c| c.starts_with(">=") || c.starts_with("~=") || c.starts_with("=="))?;
    let v: Vec<&str> = lower.trim_start_matches(['>', '=', '~']).trim().split('.').take(2).collect();
    (v.len() == 2 && v.iter().all(|x| x.chars().all(|c| c.is_ascii_digit()))).then(|| v.join("."))
}

/// How ty reads a project: where it looks for modules, the environment and the Python it reads
/// with, from the project's configuration and what is on disk.
struct Setup {
    root: SystemPathBuf,
    /// Where ty looks for modules: what the configuration names, unless a path it names is not
    /// there.
    search: SearchPathSettings,
    found: Option<PythonEnvironment>,
    /// The directory of the environment found.
    venv: Option<SystemPathBuf>,
    version: PythonVersion,
    /// Whether an environment with libraries was found.
    environment: bool,
}

impl Setup {
    /// How ty reads the project at `root`, with the search paths it reads with.
    fn of(root: &SystemPath, system: &OsSystem, vendored: &VendoredFileSystem) -> (Setup, SearchPaths) {
        let settings = Settings::of(root);
        let mut roots: Vec<SystemPathBuf> = settings.roots.iter().map(|r| root.join(r)).collect();
        if roots.is_empty() {
            // the layouts ty finds by itself: the root, `src` and maturin's `python` where they are
            // no package, and a nested `<name>/<name>`
            roots.push(root.to_path_buf());
            for dir in ["src", "python"] {
                let d = root.join(dir);
                if system.is_directory(&d) && !system.is_file(&d.join("__init__.py")) {
                    roots.push(d);
                }
            }
            if let Some(name) = settings.name.as_deref().map(|n| n.replace('-', "_")) {
                if system.is_directory(&root.join(&name).join(&name)) {
                    roots.push(root.join(&name));
                }
            }
        }
        let mut search = SearchPathSettings::new(roots);
        search.extra_paths = settings.extra_paths.iter().map(|p| root.join(p)).collect();
        let (venv, found) = match &settings.python {
            Some(p) => {
                let origin = SysPrefixPathOrigin::ConfigFileSetting(
                    Arc::new(settings.file.clone().unwrap_or_else(|| root.join("pyproject.toml"))),
                    None,
                );
                (root.join(p), PythonEnvironment::new(root.join(p), origin, system).ok())
            }
            // the project's `.venv`; a `VIRTUAL_ENV`, a conda environment or a python on `PATH` are the shell's
            None => {
                let v = root.join(".venv");
                let found = system
                    .is_directory(&v)
                    .then(|| PythonEnvironment::new(v.clone(), SysPrefixPathOrigin::LocalVenv, system).ok())
                    .flatten();
                (v, found)
            }
        };
        // the version ty reads with: the one configured, else the lowest the project supports
        // (`requires-python`), else the environment's
        let mut version = settings.python_version.as_deref().or(settings.requires.as_deref()).and_then(|v| v.parse::<PythonVersion>().ok());
        let mut environment = false;
        if let Some(env) = &found {
            if version.is_none() {
                version = env.python_version_from_metadata().map(|v| v.version);
            }
            if let Ok(paths) = env.site_packages_paths(system) {
                search.site_packages_paths = paths.into_vec();
                environment = !search.site_packages_paths.is_empty();
            }
        }
        let paths = match search.to_search_paths(system, vendored, &FallibleStrategy) {
            Ok(p) => p,
            // a path the configuration names that is not there: read without it
            Err(_) => {
                search = SearchPathSettings::new(vec![root.to_path_buf()]);
                search.to_search_paths(system, vendored, &FallibleStrategy).expect("the project root is a search path")
            }
        };
        let venv = found.is_some().then_some(venv);
        let setup = Setup { root: root.to_path_buf(), search, found, venv, version: version.unwrap_or(PythonVersion::PY313), environment };
        (setup, paths)
    }

    /// The site-packages directories, as the file system resolves them.
    fn installed(&self) -> Vec<PathBuf> {
        self.search.site_packages_paths.iter().map(|p| resolved(p.as_std_path())).collect()
    }

    /// The places ty looks for a module in, in its order, each once: the extra paths, the project's
    /// roots, the standard library, then each site-packages directory followed by the directories
    /// its `.pth` files add.
    fn places(&self) -> Vec<(PathBuf, Whose)> {
        let installed = self.installed();
        let root = resolved(self.root.as_std_path());
        let whose = |p: &Path| match () {
            _ if installed.iter().any(|s| p.starts_with(s)) => Whose::Installed,
            _ if p.starts_with(&root) => Whose::Own,
            _ => Whose::Elsewhere,
        };
        let mut out: Vec<(PathBuf, Whose)> = vec![];
        let mut add = |p: PathBuf, w: Whose| {
            if !out.iter().any(|(q, _)| *q == p) {
                out.push((p, w));
            }
        };
        for p in self.search.extra_paths.iter().chain(&self.search.src_roots) {
            let p = resolved(p.as_std_path());
            let w = whose(&p);
            add(p, w);
        }
        add(PathBuf::new(), Whose::Standard);
        for site in &installed {
            add(site.clone(), Whose::Installed);
            for pth in files_with(site, "pth") {
                let text = std::fs::read_to_string(&pth).unwrap_or_default();
                for line in text.lines().map(str::trim_end) {
                    if line.is_empty() || line.starts_with('#') || line.starts_with("import ") || line.starts_with("import\t") {
                        continue;
                    }
                    let p = resolved(&site.join(line));
                    if p.is_dir() {
                        let w = whose(&p);
                        add(p, w);
                    }
                }
            }
        }
        out
    }

    /// What ty reads a library's imports from, as checks of how each place stands: of an installed
    /// distribution, the listing of its directory (each distribution by its version) and the
    /// `.pth` files there; of the project's own code, the modules it offers, and the code of each
    /// a later place offers too, since every library that imports that name gets the project's; of
    /// code elsewhere, the modules it offers and the code of each that no place before it holds.
    fn checks(&self) -> Vec<(Whose, Check)> {
        let places = self.places();
        let offers: Vec<BTreeMap<String, Offer>> =
            places.iter().map(|(p, w)| if *w == Whose::Standard { standard(self.version) } else { offered(p) }).collect();
        let mut out = vec![];
        if let Some(v) = &self.venv {
            out.push((Whose::Installed, Check::File(v.join("pyvenv.cfg").into_std_path_buf())));
        }
        for (i, (p, whose)) in places.iter().enumerate() {
            match whose {
                Whose::Standard => {}
                Whose::Installed => {
                    out.push((Whose::Installed, Check::Listing(p.clone(), vec![BYTECODE.to_string()])));
                    out.extend(files_with(p, "pth").into_iter().map(|f| (Whose::Installed, Check::File(f))));
                }
                Whose::Own => {
                    out.push((Whose::Own, modules_in(p)));
                    let shared = offers[i].iter().filter(|(n, o)| o.regular && offers[i + 1..].iter().any(|later| later.contains_key(*n)));
                    for (_, offer) in shared {
                        out.extend(offer.code(Whose::Own));
                    }
                }
                Whose::Elsewhere => {
                    let earlier: Vec<PathBuf> = places[..i].iter().filter(|(_, w)| *w != Whose::Standard).map(|(q, _)| q.clone()).collect();
                    let held = |n: &str| offers[..i].iter().any(|o| o.get(n).map_or(false, |x| x.regular));
                    elsewhere(p, &earlier, &held, 0, &mut out);
                }
            }
        }
        out
    }

    /// Everything ty finds a library's imports in, as it stands now, with the Python it reads
    /// with: what ty says of an installed library's file holds while this does. The project's own
    /// places count by what they offer, not by where they are.
    fn fingerprint(&self) -> String {
        let mut lines = vec![format!("python {}", self.version)];
        for (whose, check) in self.checks() {
            lines.push(match whose {
                Whose::Own => format!("own {} {}", check.kind(), check.now()),
                _ => format!("{check:?} {}", check.now()),
            });
        }
        crate::cache::digest(lines.join("\n").as_bytes())
    }
}

/// What a reading of the Python project at `root` takes from outside it: where ty finds the
/// modules its code and its libraries import, as checks of how each stands.
pub fn environment(root: &Path) -> Vec<Check> {
    let Some(root) = absolute(root) else { return vec![] };
    let system = OsSystem::new(&root);
    let (setup, _) = Setup::of(&root, &system, ty_vendored::file_system());
    setup.checks().into_iter().map(|(_, c)| c).collect()
}

/// Whose the modules in a place ty looks in are.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Whose {
    /// The project's own, read with it.
    Own,
    /// ty's own typeshed: the standard library.
    Standard,
    /// An installed distribution's: its version's.
    Installed,
    /// Neither: code that changes without a version.
    Elsewhere,
}

/// What a directory offers an import under one name: a package or a module (source, stub,
/// compiled), or a portion of a namespace package.
#[derive(Default)]
struct Offer {
    regular: bool,
    paths: Vec<PathBuf>,
}

impl Offer {
    /// Checks of its code: a package's whole directory, a module's files.
    fn code(&self, whose: Whose) -> Vec<(Whose, Check)> {
        self.paths.iter().map(|p| (whose, if p.is_dir() { Check::Tree(p.clone()) } else { Check::File(p.clone()) })).collect()
    }
}

/// The extensions of the files Python imports, and the files that make a directory a package.
const SOURCES: &[&str] = &["py", "pyi", "so", "pyd"];
const MARKS: &[&str] = &["__init__.py", "__init__.pyi"];
/// Where Python writes what it compiled of a directory's modules: written whenever one is first
/// run, and never read by ty.
const BYTECODE: &str = "__pycache__";

/// How deep namespace packages are followed before the rest of one counts whole.
const NAMESPACE_DEPTH: usize = 8;

/// A check of the modules a directory offers.
fn modules_in(dir: &Path) -> Check {
    let strings = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect();
    Check::Offered(dir.to_path_buf(), Imports { sources: strings(SOURCES), marks: strings(MARKS), skip: strings(&[BYTECODE]) })
}

/// The modules a directory offers an import, by name.
fn offered(dir: &Path) -> BTreeMap<String, Offer> {
    let mut out: BTreeMap<String, Offer> = BTreeMap::new();
    let identifier =
        |s: &str| s.chars().next().map_or(false, |c| c.is_alphabetic() || c == '_') && s.chars().all(|c| c.is_alphanumeric() || c == '_');
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten().filter(|e| e.file_name() != BYTECODE) {
        let (path, file) = (e.path(), e.file_name().to_string_lossy().to_string());
        let (name, regular) = if path.is_dir() {
            match file.strip_suffix("-stubs") {
                Some(stem) => (stem.to_string(), true),
                None => (file.clone(), MARKS.iter().any(|m| path.join(m).is_file())),
            }
        } else {
            let Some((stem, ext)) = file.split_once('.') else { continue };
            if !SOURCES.iter().any(|s| ext == *s || ext.ends_with(&format!(".{s}"))) {
                continue;
            }
            (stem.to_string(), true)
        };
        if identifier(&name) {
            let o = out.entry(name).or_default();
            o.regular |= regular;
            o.paths.push(path);
        }
    }
    out
}

/// The modules of the standard library at `version`, as ty's typeshed lists them.
fn standard(version: PythonVersion) -> BTreeMap<String, Offer> {
    let text = ty_vendored::file_system().read_to_string("stdlib/VERSIONS").unwrap_or_default();
    let at = |v: &str| -> Option<(u8, u8)> {
        let (a, b) = v.trim().split_once('.')?;
        Some((a.parse().ok()?, b.parse().ok()?))
    };
    let now = (version.major, version.minor);
    let mut out = BTreeMap::new();
    for line in text.lines().filter_map(|l| l.split('#').next()) {
        let Some((module, range)) = line.split_once(':') else { continue };
        let (from, to) = range.split_once('-').unwrap_or((range, ""));
        let within = at(from).map_or(false, |f| f <= now) && (to.trim().is_empty() || at(to).map_or(false, |t| now <= t));
        if within && !module.contains('.') {
            out.insert(module.trim().to_string(), Offer { regular: true, paths: vec![] });
        }
    }
    out
}

/// The checks of code elsewhere at `dir`: the modules it offers, and the code of each that the
/// places before it (`earlier`, the same name's directories in each) do not hold. A namespace
/// package is read from all its portions in order, so its modules are asked the same way.
fn elsewhere(dir: &Path, earlier: &[PathBuf], held: &dyn Fn(&str) -> bool, depth: usize, out: &mut Vec<(Whose, Check)>) {
    out.push((Whose::Elsewhere, modules_in(dir)));
    if depth >= NAMESPACE_DEPTH {
        out.push((Whose::Elsewhere, Check::Tree(dir.to_path_buf())));
        return;
    }
    for (name, offer) in offered(dir) {
        if held(&name) {
            continue;
        }
        if offer.regular {
            out.extend(offer.code(Whose::Elsewhere));
            continue;
        }
        let portions: Vec<PathBuf> = earlier.iter().map(|e| e.join(&name)).filter(|p| p.is_dir()).collect();
        let inner: Vec<BTreeMap<String, Offer>> = portions.iter().map(|p| offered(p)).collect();
        let held = |n: &str| inner.iter().any(|o| o.get(n).map_or(false, |x| x.regular));
        elsewhere(&dir.join(&name), &portions, &held, depth + 1, out);
    }
}

/// The files of a directory with an extension, in order.
fn files_with(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().and_then(|x| x.to_str()) == Some(ext))
        .collect();
    out.sort();
    out
}

/// A path as the file system resolves it, where it is there.
fn resolved(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// ty's database over the files on disk.
#[salsa::db]
struct TyDb {
    storage: salsa::Storage<Self>,
    files: Files,
    system: OsSystem,
    vendored: VendoredFileSystem,
    rule_selection: Arc<RuleSelection>,
    analysis_settings: Arc<AnalysisSettings>,
    program_settings: ProgramSettings,
}

impl TyDb {
    /// The database for the project at `root`, and how it reads the project.
    fn new(root: &SystemPath) -> (TyDb, Setup) {
        let system = OsSystem::new(root);
        let vendored = ty_vendored::file_system().clone();
        let mut db = TyDb {
            storage: salsa::Storage::new(None),
            program_settings: ProgramSettings::empty(&vendored),
            system,
            vendored,
            files: Files::default(),
            rule_selection: Arc::new(checks()),
            analysis_settings: AnalysisSettings::default().into(),
        };
        db.files().try_add_root(&db, root, FileRootKind::Project);
        let (mut setup, search_paths) = Setup::of(root, &db.system, &db.vendored);
        search_paths.try_register_static_roots(&db);
        db.program_settings = ProgramSettings {
            python_version: PythonVersionWithSource { version: setup.version, source: PythonVersionSource::default() },
            python_platform: PythonPlatform::default(),
            search_paths,
            python_environment: Ok(setup.found.take()),
        };
        (db, setup)
    }
}

#[salsa::db]
trait ProgramDb: Db {
    fn program_settings(&self) -> &ProgramSettings;
}

#[salsa::tracked(returns(copy))]
fn program(db: &dyn ProgramDb) -> Program<'_> {
    Program::from_settings(db, db.program_settings())
}

#[salsa::db]
impl SourceDb for TyDb {
    fn vendored(&self) -> &VendoredFileSystem {
        &self.vendored
    }
    fn system(&self) -> &dyn System {
        &self.system
    }
    fn files(&self) -> &Files {
        &self.files
    }
}

#[salsa::db]
impl PythonCoreDb for TyDb {
    fn should_check_file(&self, file: File) -> bool {
        !file.path(self).is_vendored_path()
    }
}

#[salsa::db]
impl Db for TyDb {
    fn check_file(&self, file: File) -> Vec<Diagnostic> {
        check_file_unwrap(self, self.program_file(file))
    }
    fn program_file(&self, file: File) -> ProgramFile<'_> {
        program(self).program_file(self, file)
    }
    fn python_version_with_source(&self, _file: File) -> &PythonVersionWithSource {
        &self.program_settings.python_version
    }
    fn rule_selection(&self, _file: File) -> &RuleSelection {
        &self.rule_selection
    }
    fn lint_registry(&self) -> &LintRegistry {
        default_lint_registry()
    }
    fn analysis_settings(&self, _file: File) -> &AnalysisSettings {
        &self.analysis_settings
    }
    fn dependency_metadata(&self, _file: File) -> Option<&DependencyMetadata> {
        None
    }
    fn verbose(&self) -> bool {
        false
    }
    fn is_open_file(&self, _file: File) -> bool {
        false
    }
    fn dyn_clone(&self) -> Box<dyn Db> {
        unreachable!("Atlas reads types; it never drives ty's fixes")
    }
}

#[salsa::db]
impl ModuleResolverDb for TyDb {}

#[salsa::db]
impl ProgramDb for TyDb {
    fn program_settings(&self) -> &ProgramSettings {
        &self.program_settings
    }
}

#[salsa::db]
impl salsa::Database for TyDb {}

/// Walks one file and writes down what ty says of its calls and its values.
struct Reader<'a, 'db> {
    db: &'db TyDb,
    model: &'a SemanticModel<'db>,
    /// The files a call into which is surely named where it goes, by their keys.
    certain: &'a HashSet<String>,
    keys: &'a RefCell<HashMap<PathBuf, String>>,
    said: Said,
}

fn span(e: &impl Ranged) -> (usize, usize) {
    (e.range().start().to_usize(), e.range().end().to_usize())
}

impl Reader<'_, '_> {
    /// Where a definition's range lies: its file on disk, where its name is written, and the
    /// module ty resolves the file to.
    fn place(&self, range: FileRange) -> Place {
        let file = range.file();
        let (key, path) = match file.path(self.db) {
            FilePath::System(p) => {
                let k = self.keys.borrow_mut().entry(p.as_std_path().to_path_buf()).or_insert_with(|| key(p.as_std_path())).clone();
                (Some(k), Some(p.as_std_path().to_path_buf()))
            }
            _ => (None, None),
        };
        let rf = self.db.program_file(file).resolver_file(self.db);
        let module = file_to_module(self.db, rf).map(|m| m.name(self.db).to_string());
        Place { key, path, at: range.range().start().to_usize(), module }
    }

    /// Where the type `t` is defined.
    fn place_of(&self, t: Type) -> Option<Place> {
        let env = self.model.program_environment();
        Some(self.place(t.definition(self.db, &env)?.focus_range(self.db)?))
    }

    /// Whether a place is named wherever Atlas reads: in a file surely read, or in a module.
    fn sure(&self, p: &Place) -> bool {
        p.module.is_some() || p.key.as_ref().map_or(false, |k| self.certain.contains(k))
    }

    /// Whether a place may be named as a library's module.
    fn may_be_library(&self, p: &Place) -> bool {
        p.module.is_some() && !p.key.as_ref().map_or(false, |k| self.certain.contains(k))
    }

    /// What calling `func` reaches: by its type when that is a function or a class, and unless
    /// that surely names where it goes, by where its name is defined, and for an instance by its
    /// class. A decorator without a return type erases a function's type, not where it is
    /// defined; a name on a value whose type is unknown has no definition to go to.
    fn reached(&self, func: &ast::Expr) -> Reached {
        let Some(ty) = func.inferred_type(self.model) else { return Reached::default() };
        let by = match ty {
            Type::Never => return Reached::default(),
            Type::FunctionLiteral(_) | Type::BoundMethod(_) => Some(By::Call),
            Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_) => Some(By::Construct),
            _ => None,
        };
        let mut out = Reached::default();
        if let Some(by) = by {
            if let Some(p) = self.place_of(ty) {
                let sure = self.sure(&p);
                out.typed = Some((p, by));
                if sure {
                    return out;
                }
            }
        }
        let names = match func {
            ast::Expr::Name(n) => {
                definitions_for_name(self.model, n.id.as_str(), AnyNodeRef::from(n), ImportAliasResolution::ResolveAliases)
            }
            ast::Expr::Attribute(a) => definitions_for_attribute(self.model, a),
            _ => vec![],
        };
        out.named = names.iter().map(|d| self.place(d.focus_range(self.db))).collect();
        if matches!(ty, Type::NominalInstance(_)) {
            out.instance = self.place_of(ty);
        }
        out
    }

    /// The parameters of a library function, from ty's signature, each with what its
    /// annotation says about files in any of the function's overloads (pandas' `to_csv` takes
    /// `None` in one and a path in another). A parameter spans `name: annotation`, and the
    /// types written inside it are its type; a type variable named for paths (`_StrPathT`) is a
    /// path.
    fn signature(&self, ty: Type) -> Vec<Param> {
        let env = self.model.program_environment();
        let parts = ty.display(self.db, &env).to_string_parts();
        let mut params: Vec<(String, (u32, u32))> = vec![];
        let mut depth = 0;
        for (d, r) in parts.details.iter().zip(&parts.targets) {
            match d {
                TypeDetail::SignatureStart => depth += 1,
                TypeDetail::SignatureEnd => depth -= 1,
                TypeDetail::Parameter(name) if depth == 1 => params.push((name.clone(), (r.start().into(), r.end().into()))),
                _ => {}
            }
        }
        let class = |t: &Type| -> Option<String> {
            let TypeDefinition::StaticClass(_) = t.definition(self.db, &env)? else { return None };
            let r = t.definition(self.db, &env)?.focus_range(self.db)?;
            let text = ruff_db::source::source_text(self.db, r.file());
            text.as_str().get(r.range().start().to_usize()..r.range().end().to_usize()).map(String::from)
        };
        let each: Vec<Param> = params
            .into_iter()
            .map(|(name, (a, b))| {
                let inside = parts.details.iter().zip(&parts.targets).filter(|(_, r)| u32::from(r.start()) >= a && u32::from(r.end()) <= b);
                let kinds: Vec<FileArg> = inside
                    .filter_map(|(d, _)| if let TypeDetail::Type(t) = d { class(t).and_then(|c| file_of(&c)) } else { None })
                    .collect();
                let annotation =
                    parts.label.get(a as usize..b as usize).and_then(|s| s.split_once(':')).map(|(_, t)| t.trim()).unwrap_or("");
                let file = [FileArg::Path, FileArg::Reads, FileArg::Writes, FileArg::Open]
                    .into_iter()
                    .find(|k| kinds.contains(k))
                    .or_else(|| (annotation.ends_with("PathT") && !annotation.contains(' ')).then_some(FileArg::Path));
                Param { name, file }
            })
            .collect();
        let mut out: Vec<Param> = vec![];
        for p in each {
            match out.iter_mut().find(|q| q.name == p.name) {
                Some(q) => q.file = q.file.or(p.file),
                None => out.push(p),
            }
        }
        out
    }

    /// The type of `e`, unless ty does not know it, with the classes of the values it holds.
    fn shown(&self, e: &ast::Expr) -> Option<Shown> {
        let ty = e.inferred_type(self.model)?;
        if matches!(ty, Type::Dynamic(_)) {
            return None;
        }
        let env = self.model.program_environment();
        let parts = ty.display(self.db, &env).to_string_parts();
        // the classes of the values it holds, not the parameters of their classes
        let outer = outer_classes(&parts.label);
        let mut classes = vec![];
        for d in &parts.details {
            let TypeDetail::Type(t) = d else { continue };
            let Some(def @ (TypeDefinition::StaticClass(_) | TypeDefinition::DynamicClass(_))) = t.definition(self.db, &env) else {
                continue;
            };
            let Some(r) = def.focus_range(self.db) else { continue };
            let text = ruff_db::source::source_text(self.db, r.file());
            let name = text.as_str().get(r.range().start().to_usize()..r.range().end().to_usize()).unwrap_or("").to_string();
            if outer.contains(&name) {
                classes.push((self.place(r), name));
            }
        }
        let is_class = matches!(ty, Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_));
        let is_code = matches!(
            ty,
            Type::FunctionLiteral(_) | Type::BoundMethod(_) | Type::KnownBoundMethod(_) | Type::Callable(_) | Type::ModuleLiteral(_)
        );
        Some(Shown { shown: parts.label, classes, is_class, is_code })
    }

    /// What calling `func` reaches, written down at its range, with the library function's
    /// parameters where it may call one.
    fn call(&mut self, func: &ast::Expr) {
        let reached = self.reached(func);
        let mut signature = None;
        if reached.places().any(|p| self.may_be_library(p)) {
            if let Some(ty) = func.inferred_type(self.model) {
                signature = Some(self.signature(ty)).filter(|s| !s.is_empty());
            }
        }
        if reached.places().next().is_some() {
            self.said.calls.push((span(func), reached, signature));
        }
    }

    /// The type of `e` written down at its range, where ty knows it.
    fn value(&mut self, e: &ast::Expr) {
        if let Some(t) = self.shown(e) {
            self.said.values.push((span(e), t));
        }
    }
}

impl<'db> Reader<'_, 'db> {
    /// A class with what it takes for its overrides: its bases where they are defined, and its
    /// methods.
    fn class(&self, c: &ast::StmtClassDef, file: &str, path: &SystemPathBuf, key: &str) -> Class {
        let env = self.model.program_environment();
        let bases = c
            .bases()
            .iter()
            .filter_map(|b| Some(self.place(b.inferred_type(self.model)?.definition(self.db, &env)?.focus_range(self.db)?)))
            .collect();
        let methods = c
            .body
            .iter()
            .filter_map(|s| if let ast::Stmt::FunctionDef(f) = s { Some(f) } else { None })
            .map(|f| (f.name.to_string(), f.name.range.start().to_usize()))
            .collect();
        Class { file: file.to_string(), path: path.clone(), key: key.to_string(), at: c.name.range.start().to_usize(), bases, methods }
    }
}

impl<'e> Visitor<'e> for Reader<'_, '_> {
    fn visit_stmt(&mut self, stmt: &'e ast::Stmt) {
        let value = match stmt {
            ast::Stmt::Assign(a) => Some(&*a.value),
            ast::Stmt::AnnAssign(a) => a.value.as_deref(),
            _ => None,
        };
        if let Some(v) = value.filter(|v| !matches!(v, ast::Expr::Name(_))) {
            self.value(v);
        }
        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'e ast::Expr) {
        match expr {
            ast::Expr::Call(call) => {
                self.value(expr);
                self.call(&call.func);
                // a function handed to another, to be called there: `executor.submit(work)`,
                // `Thread(target=work)`
                let handed = call.arguments.args.iter().chain(call.arguments.keywords.iter().map(|k| &k.value));
                for a in handed.filter(|a| matches!(a, ast::Expr::Name(_) | ast::Expr::Attribute(_))) {
                    let reached = self.reached(a);
                    if reached.places().next().is_some() {
                        self.said.calls.push((span(a), reached, None));
                    }
                }
            }
            ast::Expr::Name(n) if n.ctx.is_load() => {
                self.value(expr);
                let defs = definitions_for_name(self.model, n.id.as_str(), AnyNodeRef::from(n), ImportAliasResolution::ResolveAliases);
                if let Some(d) = defs.first() {
                    let p = self.place(d.focus_range(self.db));
                    self.said.defs.push((span(expr), p));
                }
            }
            ast::Expr::Attribute(a) if a.ctx.is_load() => {
                if let Some(d) = definitions_for_attribute(self.model, a).first() {
                    let p = self.place(d.focus_range(self.db));
                    self.said.defs.push((span(expr), p));
                }
                // what a module's or a class's name holds: a function, a constant (`pd.read_csv`)
                if matches!(&*a.value, ast::Expr::Name(_)) {
                    self.value(expr);
                }
            }
            _ => {}
        }
        visitor::walk_expr(self, expr);
    }
}

/// Walks one file's statements for its classes.
struct Classes<'a, 'db> {
    reader: Reader<'a, 'db>,
    /// The name Atlas knows the file by, its path and its key.
    file: &'a str,
    path: &'a SystemPathBuf,
    key: String,
    out: Vec<Class>,
}

impl<'e> Visitor<'e> for Classes<'_, '_> {
    fn visit_stmt(&mut self, stmt: &'e ast::Stmt) {
        if let ast::Stmt::ClassDef(c) = stmt {
            let class = self.reader.class(c, self.file, self.path, &self.key);
            self.out.push(class);
        }
        visitor::walk_stmt(self, stmt);
    }

    /// A statement is never inside an expression.
    fn visit_expr(&mut self, _: &'e ast::Expr) {}
}

#[cfg(test)]
mod tests {
    use super::outer_classes;

    #[test]
    fn a_project_is_read_with_the_lowest_python_it_supports() {
        use super::lowest_version;
        assert_eq!(lowest_version(">=3.11").as_deref(), Some("3.11"));
        assert_eq!(lowest_version(">=3.10, <4").as_deref(), Some("3.10"));
        assert_eq!(lowest_version("~=3.12.1").as_deref(), Some("3.12"));
        assert_eq!(lowest_version("<4"), None);
    }

    #[test]
    fn a_type_is_the_classes_of_the_values_it_holds() {
        assert_eq!(outer_classes("ndarray[tuple[int], dtype[float64]]"), ["ndarray"]);
        assert_eq!(outer_classes("tuple[Figure, Axes]"), ["Figure", "Axes"]);
        assert_eq!(outer_classes("list[tuple[Station, float]]"), Vec::<String>::new());
        assert_eq!(outer_classes("tuple[Station, Store] | None"), ["Station", "Store", "None"]);
        assert_eq!(outer_classes("list[tuple[Station, ...]]"), ["Station"]);
        assert_eq!(outer_classes("list[Node] | None"), ["Node", "None"]);
        assert_eq!(outer_classes("dict[str, Series[Any]]"), ["Series"]);
        assert_eq!(outer_classes("dict[Mode, Plan]"), ["Plan"]);
        assert_eq!(outer_classes("dict[tuple[Mode, Level], list[Plan]] | None"), ["Plan", "None"]);
        assert_eq!(outer_classes("Counter[Mode]"), Vec::<String>::new());
    }

    /// A `type: ignore` with codes says the code knows, whatever the codes and wherever it stands
    /// among the comments of its line; the bare form and ty's own codes are ty's to read.
    #[test]
    fn a_type_ignore_with_codes_is_read_whatever_its_codes() {
        use super::ignores_with_codes;
        assert!(ignores_with_codes("# type: ignore[override]"));
        assert!(ignores_with_codes("#type:ignore[misc, override]"));
        assert!(ignores_with_codes("# noqa: E501 # type: ignore[override]  # reason"));
        assert!(!ignores_with_codes("# type: ignore"));
        assert!(!ignores_with_codes("# ty: ignore[invalid-method-override]"));
        assert!(!ignores_with_codes("# a type: ignore[override] in prose"));
    }
}
