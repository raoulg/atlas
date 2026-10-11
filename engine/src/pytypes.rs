//! Python's types, as ty sees them: where a call goes, and what a value is.
//!
//! Atlas reads a Python project's structure with tree-sitter and asks ty, Astral's type
//! checker, for its types. ty reads the project as its own `ty check` would: the environment
//! it finds (`VIRTUAL_ENV`, `.venv`) and the project's `[tool.ty.environment]`. The two trees
//! meet at an expression's byte range in its file.
//!
//! Where ty says a type is unknown, so does Atlas: a function without a return annotation
//! gives back `Unknown`, and nothing is guessed behind it.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ruff_db::diagnostic::Diagnostic;
use ruff_db::files::{File, FilePath, FileRange, FileRootKind, Files, system_path_to_file};
use ruff_db::parsed::parsed_module;
use ruff_db::system::{OsSystem, System, SystemPath, SystemPathBuf};
use ruff_db::vendored::VendoredFileSystem;
use ruff_db::Db as SourceDb;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast, AnyNodeRef, PythonVersion};
use ruff_text_size::Ranged;
use ty_module_resolver::{file_to_module, Db as ModuleResolverDb, FallibleStrategy, SearchPathSettings};
use ty_python_core::platform::PythonPlatform;
use ty_python_core::program::{Program, ProgramSettings};
use ty_python_core::{Db as PythonCoreDb, ProgramFile};
use ty_python_semantic::dependency::DependencyMetadata;
use ty_python_semantic::lint::{LintRegistry, RuleSelection};
use ty_python_semantic::types::{Type, TypeDefinition, TypeDetail};
use ty_python_semantic::{
    check_file_unwrap, default_lint_registry, definitions_for_attribute, definitions_for_name, AnalysisSettings, Db,
    HasType, ImportAliasResolution, PythonEnvironment, PythonVersionSource, PythonVersionWithSource, SemanticModel,
    SysPrefixPathOrigin,
};

/// Where a definition lives: in the project, at its name, or in a library's module.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// A file Atlas reads, and the byte offset of the definition's name.
    Here { file: String, at: usize },
    /// The module a library defines it in: `neo4j._sync.work.session`, `pathlib`, `builtins`.
    Lib(String),
}

/// How a call reaches its target.
#[derive(Clone, Copy, Debug, PartialEq)]
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FileArg {
    Path,
    Reads,
    Writes,
    Open,
}

/// A parameter of a library function: its name, and what it is handed when that is a file.
#[derive(Clone, Debug, PartialEq)]
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
        "IO" | "TextIO" | "BinaryIO" | "IOBase" | "RawIOBase" | "BufferedIOBase" | "TextIOBase" | "TextIOWrapper" | "BufferedReader" | "BufferedWriter" | "BufferedRandom" | "FileIO" => {
            Some(FileArg::Open)
        }
        _ => None,
    }
}

/// A value's type: as ty writes it, the classes it names (`list[Node]` names `list` and
/// `Node`), and whether the value is a class itself rather than an instance.
#[derive(Clone, Debug, PartialEq)]
pub struct Value {
    pub shown: String,
    pub classes: Vec<Target>,
    pub is_class: bool,
}

/// What ty says about one file, by byte range.
#[derive(Default, Debug)]
pub struct Facts {
    callees: HashMap<(usize, usize), Vec<Callee>>,
    values: HashMap<(usize, usize), Value>,
    defs: HashMap<(usize, usize), Target>,
    signatures: HashMap<(usize, usize), Vec<Param>>,
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
        Ty::new(root).map(|ty| ty.read(files, files)).unwrap_or_default()
    }
}

/// ty over one project, asked more than once: what it has inferred for a file is kept from one
/// question to the next.
pub struct Ty {
    db: TyDb,
    environment: bool,
}

impl Ty {
    pub fn new(root: &Path) -> Option<Ty> {
        let root = absolute(root)?;
        let (db, environment) = TyDb::new(&root);
        Some(Ty { db, environment })
    }

    /// Ask about `files`, with `project` the files Atlas reads: a call into one of them goes to
    /// its definition there, a call elsewhere into a library's module.
    pub fn read(&self, files: &[(String, PathBuf)], project: &[(String, PathBuf)]) -> Types {
        let db = &self.db;
        let named = |list: &[(String, PathBuf)]| -> HashMap<SystemPathBuf, String> {
            list.iter().filter_map(|(name, path)| absolute(path).filter(|p| matches!(p.extension(), Some("py" | "ipynb"))).map(|p| (p, name.clone()))).collect()
        };
        let here = named(project);
        let asked = named(files);
        let project: HashSet<&SystemPathBuf> = here.keys().collect();
        let mut out = Types { files: HashMap::new(), environment: self.environment, libs: vec![] };
        let libs = std::cell::RefCell::new(std::collections::BTreeMap::new());
        for (path, name) in &asked {
            let Ok(file) = system_path_to_file(db, path) else { continue };
            let pf = db.program_file(file);
            let model = SemanticModel::new(db, pf);
            let parsed = parsed_module(db, pf.python_file(db)).load(db);
            let mut reader = Reader { db, model: &model, project: &project, here: &here, libs: &libs, facts: Facts::default() };
            reader.visit_body(parsed.suite());
            out.files.insert(name.clone(), reader.facts);
        }
        out.libs = libs.into_inner().into_iter().map(|(path, module)| (module, path)).collect();
        out
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
}

impl Settings {
    fn of(root: &SystemPath) -> Settings {
        let read = |name: &str| std::fs::read_to_string(root.join(name).as_std_path()).ok().and_then(|s| s.parse::<toml::Table>().ok());
        let (table, file) = match read("ty.toml") {
            Some(t) => (t.get("environment").cloned(), root.join("ty.toml")),
            None => (
                read("pyproject.toml").and_then(|t| t.get("tool")?.get("ty")?.get("environment").cloned()),
                root.join("pyproject.toml"),
            ),
        };
        let Some(env) = table else { return Settings::default() };
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
        }
    }
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
    /// The database for the project at `root`, and whether an environment was found.
    fn new(root: &SystemPath) -> (TyDb, bool) {
        let system = OsSystem::new(root);
        let vendored = ty_vendored::file_system().clone();
        let mut db = TyDb {
            storage: salsa::Storage::new(None),
            program_settings: ProgramSettings::empty(&vendored),
            system,
            vendored,
            files: Files::default(),
            rule_selection: Arc::new(RuleSelection::from_registry(default_lint_registry())),
            analysis_settings: AnalysisSettings::default().into(),
        };
        db.files().try_add_root(&db, root, FileRootKind::Project);
        let settings = Settings::of(root);
        let mut roots: Vec<SystemPathBuf> = settings.roots.iter().map(|r| root.join(r)).collect();
        if roots.is_empty() {
            roots.push(root.to_path_buf());
            if db.system.is_directory(&root.join("src")) {
                roots.push(root.join("src"));
            }
        }
        let mut search = SearchPathSettings::new(roots);
        search.extra_paths = settings.extra_paths.iter().map(|p| root.join(p)).collect();
        let found = match &settings.python {
            Some(p) => {
                let origin = SysPrefixPathOrigin::ConfigFileSetting(Arc::new(settings.file.clone().unwrap_or_else(|| root.join("pyproject.toml"))), None);
                PythonEnvironment::new(root.join(p), origin, &db.system).ok()
            }
            None => PythonEnvironment::discover(Some(root), &db.system).ok().flatten(),
        };
        let mut version = settings.python_version.as_deref().and_then(|v| v.parse::<PythonVersion>().ok());
        let mut environment = false;
        if let Some(env) = &found {
            if version.is_none() {
                version = env.python_version_from_metadata().map(|v| v.version);
            }
            if let Ok(paths) = env.site_packages_paths(&db.system) {
                search.site_packages_paths = paths.into_vec();
                environment = !search.site_packages_paths.is_empty();
            }
        }
        let search_paths = match search.to_search_paths(&db.system, &db.vendored, &FallibleStrategy) {
            Ok(p) => p,
            // a path the configuration names that is not there: read without it
            Err(_) => SearchPathSettings::new(vec![root.to_path_buf()])
                .to_search_paths(&db.system, &db.vendored, &FallibleStrategy)
                .expect("the project root is a search path"),
        };
        search_paths.try_register_static_roots(&db);
        db.program_settings = ProgramSettings {
            python_version: PythonVersionWithSource { version: version.unwrap_or(PythonVersion::PY313), source: PythonVersionSource::default() },
            python_platform: PythonPlatform::default(),
            search_paths,
            python_environment: Ok(found),
        };
        (db, environment)
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

/// Walks one file and asks ty about its calls and its values.
struct Reader<'a, 'db> {
    db: &'db TyDb,
    model: &'a SemanticModel<'db>,
    project: &'a HashSet<&'a SystemPathBuf>,
    here: &'a HashMap<SystemPathBuf, String>,
    /// The library files calls go into, with their modules' names.
    libs: &'a std::cell::RefCell<std::collections::BTreeMap<PathBuf, String>>,
    facts: Facts,
}

fn span(e: &impl Ranged) -> (usize, usize) {
    (e.range().start().to_usize(), e.range().end().to_usize())
}

impl Reader<'_, '_> {
    /// Where a definition's range lies: a file Atlas reads, or a library's module.
    fn target(&self, range: FileRange) -> Option<Target> {
        let file = range.file();
        if let FilePath::System(p) = file.path(self.db) {
            if self.project.contains(&p.to_path_buf()) {
                return Some(Target::Here { file: self.here[&p.to_path_buf()].clone(), at: range.range().start().to_usize() });
            }
        }
        let rf = self.db.program_file(file).resolver_file(self.db);
        file_to_module(self.db, rf).map(|m| Target::Lib(m.name(self.db).to_string()))
    }

    fn defined(&self, t: Type) -> Option<Target> {
        let env = self.model.program_environment();
        self.target(t.definition(self.db, &env)?.focus_range(self.db)?)
    }

    /// Where a call goes, noting the library file it goes into.
    fn called(&self, range: FileRange) -> Option<Target> {
        let target = self.target(range)?;
        if let (Target::Lib(module), FilePath::System(p)) = (&target, range.file().path(self.db)) {
            self.libs.borrow_mut().insert(p.as_std_path().to_path_buf(), module.clone());
        }
        Some(target)
    }

    fn called_type(&self, t: Type) -> Option<Target> {
        let env = self.model.program_environment();
        self.called(t.definition(self.db, &env)?.focus_range(self.db)?)
    }

    /// Where calling `func` goes: by its type when that is a function or a class, else by
    /// where its name is defined, and for an instance also its class. A decorator without a
    /// return type erases a function's type, not where it is defined; a name on a value whose
    /// type is unknown has no definition to go to.
    fn callees(&self, func: &ast::Expr) -> Vec<Callee> {
        let Some(ty) = func.inferred_type(self.model) else { return vec![] };
        let by = match ty {
            Type::Never => return vec![],
            Type::FunctionLiteral(_) | Type::BoundMethod(_) => Some(By::Call),
            Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_) => Some(By::Construct),
            _ => None,
        };
        if let Some(by) = by {
            if let Some(target) = self.called_type(ty) {
                return vec![Callee { target, by }];
            }
        }
        let names = match func {
            ast::Expr::Name(n) => definitions_for_name(self.model, n.id.as_str(), AnyNodeRef::from(n), ImportAliasResolution::ResolveAliases),
            ast::Expr::Attribute(a) => definitions_for_attribute(self.model, a),
            _ => vec![],
        };
        let mut out: Vec<Callee> = names
            .iter()
            .filter_map(|d| self.called(d.focus_range(self.db)))
            .map(|target| Callee { target, by: By::Name })
            .collect();
        if matches!(ty, Type::NominalInstance(_)) {
            out.extend(self.called_type(ty).map(|target| Callee { target, by: By::Instance }));
        }
        out.dedup();
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
                let kinds: Vec<FileArg> = inside.filter_map(|(d, _)| if let TypeDetail::Type(t) = d { class(t).and_then(|c| file_of(&c)) } else { None }).collect();
                let annotation = parts.label.get(a as usize..b as usize).and_then(|s| s.split_once(':')).map(|(_, t)| t.trim()).unwrap_or("");
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

    /// The type of `e`, unless ty does not know it.
    fn value(&self, e: &ast::Expr) -> Option<Value> {
        let ty = e.inferred_type(self.model)?;
        if matches!(ty, Type::Dynamic(_)) {
            return None;
        }
        let env = self.model.program_environment();
        let parts = ty.display(self.db, &env).to_string_parts();
        let classes = parts
            .details
            .iter()
            .filter_map(|d| match d {
                TypeDetail::Type(t) => match t.definition(self.db, &env)? {
                    TypeDefinition::StaticClass(_) | TypeDefinition::DynamicClass(_) => self.defined(*t),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        let is_class = matches!(ty, Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_));
        Some(Value { shown: parts.label, classes, is_class })
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
            if let Some(t) = self.value(v) {
                self.facts.values.insert(span(v), t);
            }
        }
        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'e ast::Expr) {
        match expr {
            ast::Expr::Call(call) => {
                if let Some(t) = self.value(expr) {
                    self.facts.values.insert(span(expr), t);
                }
                let reached = self.callees(&call.func);
                if reached.iter().any(|c| matches!(c.target, Target::Lib(_))) {
                    if let Some(ty) = call.func.inferred_type(self.model) {
                        let sig = self.signature(ty);
                        if !sig.is_empty() {
                            self.facts.signatures.insert(span(&*call.func), sig);
                        }
                    }
                }
                if !reached.is_empty() {
                    self.facts.callees.insert(span(&*call.func), reached);
                }
                // a function handed to another, to be called there: `executor.submit(work)`,
                // `Thread(target=work)`
                let handed = call.arguments.args.iter().chain(call.arguments.keywords.iter().map(|k| &k.value));
                for a in handed.filter(|a| matches!(a, ast::Expr::Name(_) | ast::Expr::Attribute(_))) {
                    let reached = self.callees(a);
                    if !reached.is_empty() {
                        self.facts.callees.insert(span(a), reached);
                    }
                }
            }
            ast::Expr::Name(n) if n.ctx.is_load() => {
                if let Some(t) = self.value(expr) {
                    self.facts.values.insert(span(expr), t);
                }
                let defs = definitions_for_name(self.model, n.id.as_str(), AnyNodeRef::from(n), ImportAliasResolution::ResolveAliases);
                if let Some(t) = defs.first().and_then(|d| self.target(d.focus_range(self.db))) {
                    self.facts.defs.insert(span(expr), t);
                }
            }
            ast::Expr::Attribute(a) if a.ctx.is_load() => {
                if let Some(t) = definitions_for_attribute(self.model, a).first().and_then(|d| self.target(d.focus_range(self.db))) {
                    self.facts.defs.insert(span(expr), t);
                }
            }
            _ => {}
        }
        visitor::walk_expr(self, expr);
    }
}
