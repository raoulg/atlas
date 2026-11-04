//! What Atlas keeps between readings is what a reading anew gives: the second reading is served
//! from the cache and draws the same graph, and a library whose code changes without a new version
//! is read anew.
//!
//! The cache keeps a working-tree part while its sources and environment stand as they did, what ty
//! says of an installed Python library's file by its bytes and environment, and a revision's parts
//! by its commit and external inputs.

use std::path::{Path, PathBuf};
use std::process::Command;

use atlas_engine::cache::Cache;
use atlas_engine::{changes, parts, Graph, Options};

fn write(dir: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=atlas", "-c", "user.email=atlas@example.org", "-c", "commit.gpgsign=false"])
        .args(args)
        .status()
        .expect("git runs")
        .success();
    assert!(ok, "git {args:?}");
}

/// A directory of its own for one test, empty.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("atlas-cache-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Options that keep what does not change in `dir`.
fn cached(dir: &Path) -> Options {
    Options { cache: Some(Cache::at(dir, "test")), ..Options::default() }
}

/// Parts as the graph's JSON writes them, with the steps each body runs (where its steps are
/// written decides them, which the JSON leaves out), and the directory a revision was written to
/// named alike, since every reading writes it anew.
fn shown(parts: &[Graph]) -> String {
    let flows: Vec<serde_json::Value> = parts.iter().map(|g| serde_json::to_value(atlas_engine::flow::of(g)).unwrap()).collect();
    let text = serde_json::to_string(&(parts, flows)).unwrap();
    regex::Regex::new(r"atlas-(revision|vision)-\d+-\d+").unwrap().replace_all(&text, "atlas-$1").to_string()
}

/// A crate the app depends on by a path outside the repository, written by its absolute path, so
/// a revision reads it from where it lies as well: it reads the file it is handed.
fn filekit(at: &Path, body: &str) {
    write(
        at,
        &[
            ("Cargo.toml", "[package]\nname = \"filekit\"\nversion = \"1.2.3\"\nedition = \"2021\"\n"),
            ("src/lib.rs", &format!("pub fn read_config(path: &str) -> String {{\n{body}\n}}\n")),
        ],
    );
}

/// A package the app's web part calls, in its `node_modules`, committed with it.
fn textkit(repo: &Path, body: &str) {
    write(
        repo,
        &[
            ("web/node_modules/textkit/package.json", r#"{ "name": "textkit", "version": "2.0.0", "main": "index.js" }"#),
            (
                "web/node_modules/textkit/index.js",
                &format!("import {{ readFileSync }} from \"node:fs\";\n\nexport function load(path) {{\n{body}\n}}\n"),
            ),
        ],
    );
}

/// A repository of two parts that read libraries, committed: a crate that depends on a crate by
/// its path outside the repository, and a package that calls one in its `node_modules`.
fn project(dir: &Path) -> PathBuf {
    let (repo, kit) = (dir.join("app"), dir.join("filekit"));
    filekit(&kit, "    std::fs::read_to_string(path).unwrap_or_default()");
    write(
        &repo,
        &[
            (
                "Cargo.toml",
                &format!(
                    "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nfilekit = {{ path = {:?} }}\n",
                    kit.to_string_lossy()
                ),
            ),
            ("src/main.rs", "fn main() {\n    let text = filekit::read_config(\"settings.toml\");\n    println!(\"{text}\");\n}\n"),
            ("web/package.json", r#"{ "name": "web" }"#),
            ("web/tsconfig.json", "{}"),
            (
                "web/src/main.ts",
                "import { load } from \"textkit\";\n\nexport function main(): string {\n  return load(\"page.html\");\n}\n",
            ),
        ],
    );
    textkit(&repo, "  return readFileSync(path, \"utf8\");");
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "app"]);
    repo
}

/// The places a part's library function touches, by kind and operation.
fn touched(parts: &[Graph], id: &str) -> Vec<(String, String)> {
    parts.iter().filter_map(|g| g.node(id)).flat_map(|n| &n.touches).map(|t| (t.kind_name().to_string(), t.op.clone())).collect()
}

/// A revision is read once: the second reading of the same commit is the one kept, and draws the
/// same graph as a reading anew. The crate the revision depends on by a path changes without a new
/// version: the reading kept saw its code as it stood, so the revision is read anew, and draws
/// what the crate's code now does. A package in the commit's own `node_modules` is the commit's,
/// so a change to it in the working tree changes the working tree's graph and not the revision's.
#[test]
fn a_revision_is_read_once_and_anew_when_its_libraries_change() {
    let dir = scratch("revision");
    let repo = project(&dir);
    let opts = cached(&dir.join("cache"));
    let fresh = || changes::parts_at(&repo, Some("HEAD"), &Options::default()).unwrap();
    let first = changes::parts_at(&repo, Some("HEAD"), &opts).unwrap();
    assert_eq!(opts.cache.as_ref().unwrap().hits(), 0, "the first reading reads");
    let second = changes::parts_at(&repo, Some("HEAD"), &opts).unwrap();
    assert_eq!(opts.cache.as_ref().unwrap().hits(), 1, "the second reading is the one kept");
    assert_eq!(shown(&second), shown(&first));
    assert_eq!(shown(&second), shown(&fresh()), "what is kept is what a reading anew gives");
    assert!(second.iter().any(|g| g.node("filekit.read_config").map_or(false, |n| n.library)), "the crate is read in the revision");
    assert!(second.iter().any(|g| g.node("textkit.load").map_or(false, |n| n.library)), "the package is read in the revision");

    // the crate's code changes, its version does not
    filekit(&dir.join("filekit"), "    std::fs::write(path, \"\").unwrap();\n    String::new()");
    let third = changes::parts_at(&repo, Some("HEAD"), &opts).unwrap();
    assert_eq!(opts.cache.as_ref().unwrap().hits(), 2, "the kept reading is found");
    assert_eq!(shown(&third), shown(&fresh()), "and not used: the crate it read changed");
    assert!(
        touched(&third, "filekit.read_config").contains(&("disk_write".into(), "write".into())),
        "{:?}",
        touched(&third, "filekit.read_config")
    );

    // the package's code changes in the working tree, its version does not
    textkit(&repo, "  return readFileSync(path, \"latin1\").trim();");
    let fourth = changes::parts_at(&repo, Some("HEAD"), &opts).unwrap();
    assert_eq!(opts.cache.as_ref().unwrap().hits(), 3, "the reading kept after the crate changed");
    assert_eq!(shown(&fourth), shown(&third), "the commit's package is the commit's");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A crate and a package in the working tree are kept with their source and library inputs. A
/// change to either library's code without a new version is seen on the next reading.
#[test]
fn the_working_tree_reuses_current_parts_and_refreshes_changed_libraries() {
    let dir = scratch("worktree");
    let repo = project(&dir);
    let opts = cached(&dir.join("cache"));
    let read = || parts::analyze_parts(&[repo.clone()], &opts).unwrap();
    let (first, second) = (read(), read());
    assert_eq!(shown(&first), shown(&second));
    assert!(opts.cache.as_ref().unwrap().hits() >= 2, "both parts are found in the index");
    assert_eq!(opts.cache.as_ref().unwrap().writes(), 2, "the warm reading writes no part");
    filekit(&dir.join("filekit"), "    std::fs::write(path, \"\").unwrap();\n    String::new()");
    textkit(&repo, "  return readFileSync(path, \"latin1\").trim();");
    let third = read();
    assert!(touched(&third, "filekit.read_config").contains(&("disk_write".into(), "write".into())));
    assert_eq!(shown(&third), shown(&parts::analyze_parts(&[repo.clone()], &Options::default()).unwrap()));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A Python project with its own environment, `.venv`, in which `kit` is installed and imports
/// `helper`, installed beside it.
fn python(dir: &Path) -> PathBuf {
    let repo = dir.join("pyapp");
    write(
        &repo,
        &[
            ("pyproject.toml", "[project]\nname = \"pyapp\"\nversion = \"0\"\nrequires-python = \">=3.13\"\n"),
            ("pyapp/__init__.py", ""),
            ("pyapp/main.py", "from kit import read_config\n\n\ndef run(path: str) -> str:\n    return read_config(path)\n"),
            (".venv/pyvenv.cfg", "home = /usr/bin\nversion_info = 3.13.0\ninclude-system-site-packages = false\n"),
        ],
    );
    kit(&repo, "    return open(path).read()");
    helper(&repo, "1.0", "int");
    repo
}

/// Where the environment keeps its packages: `Lib\site-packages` on Windows, else under the
/// Python version.
fn site(repo: &Path) -> PathBuf {
    if cfg!(windows) {
        repo.join(".venv").join("Lib").join("site-packages")
    } else {
        repo.join(".venv/lib/python3.13/site-packages")
    }
}

/// `kit` at version 1.0, its `read_config` written as `body`.
fn kit(repo: &Path, body: &str) {
    let site = site(repo);
    write(
        &site,
        &[
            ("kit-1.0.dist-info/METADATA", "Name: kit\nVersion: 1.0\n"),
            ("kit/__init__.py", &format!("from helper import size\n\n\ndef read_config(path):\n    n = size()\n{body}\n")),
        ],
    );
}

/// `helper` at `version`, its `size` giving `ty`.
fn helper(repo: &Path, version: &str, ty: &str) {
    let site = site(repo);
    for e in std::fs::read_dir(&site).into_iter().flatten().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("helper-")) {
        let _ = std::fs::remove_dir_all(e.path());
    }
    write(
        &site,
        &[
            (&format!("helper-{version}.dist-info/METADATA"), &format!("Name: helper\nVersion: {version}\n")),
            ("helper.py", &format!("def size() -> {ty}:\n    ...\n")),
        ],
    );
}

/// What ty says of an installed library's file is kept: the second reading is served from the
/// cache and draws the same graph. The file's code changing without a new version is seen, by its
/// bytes; another version of a library it imports is seen, by the environment it is read in.
#[test]
fn what_ty_says_of_a_library_is_kept_by_its_code_and_its_environment() {
    let dir = scratch("python");
    let repo = python(&dir);
    let opts = cached(&dir.join("cache"));
    let read = |o: &Options| serde_json::to_string(&atlas_engine::analyze(&repo, o).unwrap()).unwrap();
    let first = read(&opts);
    assert!(first.contains("\"kit.read_config\""), "the library's function is read");
    assert_eq!(opts.cache.as_ref().unwrap().hits(), 0);
    let second = read(&opts);
    assert!(opts.cache.as_ref().unwrap().hits() > 0, "the second reading is served from the cache");
    assert_eq!(second, first);
    assert_eq!(second, read(&Options::default()), "what is kept is what a reading anew gives");

    // the library's code changes, its version does not
    kit(&repo, "    open(path, \"w\").write(\"\")\n    return \"\"");
    let changed = read(&opts);
    assert_eq!(changed, read(&Options::default()));
    assert_ne!(changed, first, "the change is seen");

    // a library it imports is installed at another version
    helper(&repo, "2.0", "str");
    assert_eq!(read(&opts), read(&Options::default()));
    let _ = std::fs::remove_dir_all(&dir);
}

/// One small program in each language, its manifest at the top of its repository, with a folder of
/// documentation: its files, and a file of its code that a reader of the language lists where it is
/// added. (A new folder at the top of a Python project is a module an import may find, a namespace
/// package; one inside `docs` is not.)
type Program = (&'static str, &'static [(&'static str, &'static str)], (&'static str, &'static str));

const PROGRAMS: &[Program] = &[
    (
        "rust",
        &[
            ("docs/index.md", "# tiny\n"),
            ("Cargo.toml", "[package]\nname = \"tiny\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
            ("src/main.rs", "fn main() {\n    std::fs::write(\"out.txt\", \"1\").unwrap();\n}\n"),
        ],
        ("src/extra.rs", "pub fn extra() -> u32 {\n    1\n}\n"),
    ),
    (
        "python",
        &[
            ("docs/index.md", "# tiny\n"),
            ("pyproject.toml", "[project]\nname = \"tiny\"\nversion = \"0.1.0\"\n"),
            ("src/tiny/__init__.py", "def save(path: str) -> None:\n    open(path, \"w\").write(\"1\")\n"),
        ],
        ("src/tiny/extra.py", "def extra() -> int:\n    return 1\n"),
    ),
    (
        "typescript",
        &[
            ("docs/index.md", "# tiny\n"),
            ("package.json", r#"{ "name": "tiny" }"#),
            ("tsconfig.json", "{}"),
            ("src/main.ts", "export function main(): string {\n  return \"1\";\n}\n"),
        ],
        ("src/extra.ts", "export function extra(): number {\n  return 1;\n}\n"),
    ),
];

/// Files no reader of any language asks about: a picture in a new folder of the documentation, and
/// a note.
const UNREAD: &[(&str, &str)] = &[("docs/img/x.svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"/>\n"), ("notes.txt", "a note\n")];

/// A reading depends only on what its reader asked of the repository. A file no question covers
/// leaves the part's entry kept, and the graph the one read before and the one a reading anew gives;
/// a file of the language's code where its reader lists the code is read anew, and the graph is the
/// one a reading anew gives. Each language's reader asks its questions of the listing alike.
#[test]
fn a_reading_rests_only_on_what_its_reader_asked() {
    let dir = scratch("asked");
    for (language, files, code) in PROGRAMS {
        let repo = dir.join(language);
        write(&repo, files);
        let opts = cached(&dir.join(format!("cache-{language}")));
        let cache = opts.cache.as_ref().unwrap();
        let read = || parts::analyze_parts(&[repo.clone()], &opts).unwrap();
        let anew = || parts::analyze_parts(&[repo.clone()], &Options::default()).unwrap();
        let first = read();
        let writes = cache.writes();
        assert!(writes > 0, "{language}: the first reading is kept");

        write(&repo, UNREAD);
        let second = read();
        assert_eq!(cache.writes(), writes, "{language}: a file no question covers keeps the entry");
        assert_eq!(shown(&second), shown(&first), "{language}: and the graph");
        assert_eq!(shown(&second), shown(&anew()), "{language}: which is what a reading anew gives");

        write(&repo, &[*code]);
        let third = read();
        assert!(cache.writes() > writes, "{language}: a file of code where the reader lists it is read anew");
        assert_eq!(shown(&third), shown(&anew()), "{language}: and gives what a reading anew gives");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// What a reading rests on holds while the repository answers it alike: a file no question covers,
/// or a file read written again with the same bytes, leaves it holding; a file of code where the
/// reader lists it, other bytes in a file read, or another `.atlasignore`, does not.
#[test]
fn a_reading_holds_until_what_it_rests_on_changes() {
    let dir = scratch("holds");
    for (language, files, code) in PROGRAMS {
        let repo = dir.join(language);
        write(&repo, files);
        let witnessed = || parts::witnessed(&[repo.clone()], &Options::default()).unwrap().1;
        let mut held = witnessed();
        assert!(held.hold(), "{language}: a reading holds of the repository it read");
        write(&repo, UNREAD);
        assert!(held.hold(), "{language}: a file no question covers leaves it holding");
        write(&repo, files);
        assert!(held.hold(), "{language}: so do the same bytes written again");
        write(&repo, &[(".atlasignore", "notes.txt  # not code\n")]);
        assert!(!held.hold(), "{language}: another .atlasignore does not");

        let mut held = witnessed();
        write(&repo, &[*code]);
        assert!(!held.hold(), "{language}: a file of code where the reader lists it does not");

        let mut held = witnessed();
        let (file, text) = files[files.len() - 1];
        write(&repo, &[(file, &format!("{text}\n"))]);
        assert!(!held.hold(), "{language}: other bytes in a file read do not");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
