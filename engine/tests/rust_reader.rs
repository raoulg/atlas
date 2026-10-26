//! Rust is read by Rust's rules, in any crate: what a path names in each namespace, what a value
//! holds through a pattern or a variant, the value a call gives as a receiver, what a glob brings
//! in, and what a command line derived with clap holds.

use atlas_engine::{analyze, EdgeKind, EntryKind, Graph, Options};

/// A crate written to a directory of its own, read, and removed.
fn written(name: &str, files: &[(&str, &str)]) -> Graph {
    let dir = std::env::temp_dir().join(format!("atlas-rust-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let manifest = format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
    for (path, text) in std::iter::once(&("Cargo.toml", manifest.as_str())).chain(files) {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    let g = analyze(&dir, &Options::default());
    let _ = std::fs::remove_dir_all(&dir);
    g.expect("the crate parses")
}

/// A function's touches as (kind, operation, place, the functions that make it whole).
fn places(g: &Graph, id: &str) -> Vec<(String, String, String, Vec<String>)> {
    let mut out: Vec<_> = g
        .node(id)
        .unwrap_or_else(|| panic!("no node {id}"))
        .touches
        .iter()
        .map(|t| (t.kind_name().to_string(), t.op.clone(), t.artifact.clone(), t.by.clone()))
        .collect();
    out.sort();
    out
}

fn place(kind: &str, op: &str, artifact: &str, by: &[&str]) -> (String, String, String, Vec<String>) {
    (kind.to_string(), op.to_string(), artifact.to_string(), by.iter().map(|b| b.to_string()).collect())
}

/// Isomorphic is the same: a line written to `io::stderr()` or `io::stdout()` is shown to the human
/// as `eprintln!` and `println!` show it, and the stream the human types into is read from.
#[test]
fn a_standard_stream_is_the_screen() {
    let g = written(
        "streams",
        &[(
            "src/main.rs",
            "use std::io::Write;\n\nfn said(line: &str) {\n    let mut terminal = std::io::stderr();\n    let _ = writeln!(terminal, \"{line}\");\n}\n\nfn shown(line: &str) {\n    let mut out = std::io::stdout();\n    let _ = writeln!(out, \"{line}\");\n}\n\nfn printed(line: &str) {\n    eprintln!(\"{line}\");\n}\n\nfn main() {\n    said(\"a\");\n    shown(\"b\");\n    printed(\"c\");\n}\n",
        )],
    );
    let screen = |id: &str| places(&g, id).iter().filter(|p| p.0 == "screen").count();
    assert_eq!((screen("streams.said"), screen("streams.shown"), screen("streams.printed")), (1, 1, 1), "{:?}", places(&g, "streams.said"));
}

/// Rust keeps modules apart from values: `pub use fetch::fetch` re-exports the function from the
/// module of that name, a call of `fetch` calls the function, and the head of `fetch::fetch` is
/// the module.
#[test]
fn a_name_is_a_module_or_a_value_by_where_it_stands() {
    let g = written(
        "reexport",
        &[
            ("src/lib.rs", "mod fetch;\npub use fetch::fetch;\n\npub fn twice() {\n    fetch::fetch();\n    fetch();\n}\n"),
            ("src/fetch.rs", "pub fn fetch() {}\n"),
            ("src/main.rs", "use reexport::fetch;\n\nfn main() {\n    fetch();\n}\n"),
        ],
    );
    assert_eq!(
        g.node("reexport.fetch.fetch").and_then(|n| n.entry.as_ref()).map(|e| (e.kind, e.label.as_str())),
        Some((EntryKind::Export, "fetch"))
    );
    let steps = |id: &str| g.node(id).unwrap().sites.iter().map(|s| s.to.clone()).collect::<Vec<_>>();
    assert_eq!(steps("reexport.main.main"), ["reexport.fetch.fetch"]);
    assert_eq!(steps("reexport.twice"), ["reexport.fetch.fetch", "reexport.fetch.fetch"]);
}

/// A method runs on the value its receiver is, also the value a call gives: a trait's own method
/// called on a type that implements it runs on that type, as when a name holds the value. A
/// method the type does not have is the operation of what the call gives it in (`Option`'s).
#[test]
fn a_method_runs_on_the_value_a_call_gives() {
    let g = written(
        "receiver",
        &[(
            "src/lib.rs",
            "pub trait Shape {\n    fn area(&self) -> f64;\n    fn show(&self) -> String { format!(\"{}\", self.area()) }\n}\n\npub struct Square;\n\nimpl Square {\n    pub fn new() -> Square { Square }\n    pub fn find(name: &str) -> Option<Square> { None }\n}\n\nimpl Shape for Square {\n    fn area(&self) -> f64 { 1.0 }\n}\n\npub fn both() -> f64 {\n    Square::new().show();\n    let s = Square::new();\n    s.show();\n    let a = Square::find(\"x\").map_or(0.0, |s| s.area());\n    a\n}\n",
        )],
    );
    let both = &g.node("receiver.both").unwrap().sites;
    let shows: Vec<&str> = both.iter().filter(|s| s.to == "receiver.Shape.show").map(|s| s.on.as_str()).collect();
    assert_eq!(shows, ["receiver.Square", "receiver.Square"], "{both:#?}");
    assert!(both.iter().any(|s| s.op == "map_or" && s.gives.iter().any(|g| g.name == "a")), "{both:#?}");
}

/// A name a pattern binds holds the value `Some` or `Ok` holds: `if let` in the block it opens,
/// `let .. else` after it, a `match` arm in its arm. `Ok(x)` and `Some(x)` give `x`.
#[test]
fn a_pattern_binds_what_a_variant_holds() {
    let g = written(
        "patterns",
        &[(
            "src/lib.rs",
            "use std::fs;\nuse std::path::{Path, PathBuf};\n\nfn kept(to: &Path) -> Result<PathBuf, std::io::Error> {\n    if let Some(dir) = to.parent() {\n        fs::create_dir_all(dir)?;\n    }\n    let Some(name) = to.file_name() else { return Ok(to.to_path_buf()) };\n    match to.parent() {\n        Some(up) => fs::write(up.join(\"seen\"), \"\")?,\n        None => {}\n    }\n    Ok(to.to_path_buf())\n}\n\nfn read(p: &Path) -> String {\n    fs::read_to_string(p).unwrap_or_default()\n}\n\npub fn run() {\n    read(&kept(Path::new(\"out/data.csv\")).unwrap());\n}\n",
        )],
    );
    let run = ["patterns.run"];
    // `create_dir_all(dir)` makes `out/`, the directory of the file written: part of writing it
    assert_eq!(places(&g, "patterns.kept"), [place("disk_write", "write", "out/seen", &run)]);
    assert_eq!(
        places(&g, "patterns.read"),
        [place("disk_read", "read_to_string", "out/data.csv", &run)],
        "the path `kept` gives inside `Ok(..)`"
    );
}

/// A name of one segment that the module neither declares nor imports by name is one its library
/// glob brings in; with two globs the code does not say which, and a type parameter is the
/// function's own.
#[test]
fn a_library_glob_brings_in_its_names() {
    let g = written(
        "globs",
        &[
            ("src/lib.rs", "mod one;\nmod two;\n"),
            ("src/one.rs", "use canvas::prelude::*;\n\npub fn draw<B: Backend>(area: &Area<B>) {}\n"),
            ("src/two.rs", "use canvas::prelude::*;\nuse paint::prelude::*;\n\npub fn draw(area: &Area) {}\n"),
        ],
    );
    let takes = |f: &str| g.edges.iter().filter(|e| e.kind == EdgeKind::Takes && e.src == f).map(|e| e.dst.clone()).collect::<Vec<_>>();
    assert_eq!(takes("globs.one.draw"), ["ext:canvas.prelude.Area"]);
    assert_eq!(takes("globs.two.draw"), Vec::<String>::new());
}

/// Rust fixes the type of a `let` that writes none by how the value is used: a value Atlas cannot
/// type, handed to a parameter of one of the program's functions, holds that parameter's type,
/// read in the module that declares the function (here through its glob `use canvas::prelude::*`,
/// which the caller does not have). A class of the program's is the class the `let` holds, so its
/// methods run on it; a library's type that the parameter only holds (`&[(Area<Rgb>, u8)]`) is no
/// type of the value.
#[test]
fn a_let_holds_the_type_of_the_parameter_it_is_handed_to() {
    let g = written(
        "handed",
        &[
            (
                "src/lib.rs",
                "mod paint;\n\npub struct Sheet {\n    pub n: u8,\n}\n\nimpl Sheet {\n    pub fn size(&self) -> u8 {\n        self.n\n    }\n}\n\npub fn save() {\n    let area = canvas::Backend::new().into_area();\n    let sheet = sheets::first();\n    let rows = Vec::new();\n    paint::draw(&area, &sheet);\n    paint::pairs(&rows);\n    sheet.size();\n}\n",
            ),
            ("src/paint.rs", "use canvas::prelude::*;\n\nuse crate::Sheet;\n\npub fn draw<B: Backend>(area: &Area<B>, sheet: &Sheet) {}\n\npub fn pairs(rows: &[(Area<Rgb>, u8)]) {}\n"),
        ],
    );
    let save = &g.node("handed.save").unwrap().sites;
    let values = |hs: &[atlas_engine::Held]| hs.iter().map(|h| (h.name.clone(), h.ty.clone())).collect::<Vec<_>>();
    let pair = |name: &str, ty: &str| (name.to_string(), ty.to_string());
    let hands = |to: &str| values(&save.iter().find(|s| s.to == to).unwrap_or_else(|| panic!("no step to {to}: {save:#?}")).hands);
    assert_eq!(hands("handed.paint.draw"), [pair("area", "ext:canvas.prelude.Area"), pair("sheet", "handed.Sheet")]);
    assert_eq!(hands("handed.paint.pairs"), [pair("rows", "")]);
    let made = save.iter().find(|s| s.op == "into_area").expect("the step that makes the area");
    assert_eq!(values(&made.gives), [pair("area", "ext:canvas.prelude.Area")]);
    assert!(save.iter().any(|s| s.to == "handed.Sheet.size"), "{save:#?}");
}

/// clap's options are read from the type it derives the command line from: where the command line
/// leaves an option out, a field holds the default its attribute declares, through a `let`, a
/// struct pattern and a subcommand's variant. A value of that type handed in as a parameter holds
/// what its callers hand, and a derive of the same name from another crate parses nothing.
#[test]
fn clap_options_hold_their_defaults() {
    let g = written(
        "options",
        &[(
            "src/main.rs",
            r#"use std::fs;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

const OUT: &str = "out/graph.json";

#[derive(Parser)]
struct Cli {
    /// Where the log goes.
    #[arg(long, default_value = "out/run.log")]
    log: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Write {
        #[arg(short, long, default_value = OUT)]
        out: PathBuf,
    },
}

#[derive(pest_derive::Parser)]
struct Grammar {
    #[arg(default_value = "grammar.txt")]
    file: PathBuf,
}

fn log(cli: &Cli, body: &str) {
    fs::write(&cli.log, body).unwrap();
}

fn grammar(g: Grammar, body: &str) {
    fs::write(&g.file, body).unwrap();
}

fn main() {
    let body = String::new();
    let cli = Cli::parse();
    fs::write(&cli.log, &body).unwrap();
    let Cli { log: kept, .. } = Cli::parse();
    fs::write(&kept, &body).unwrap();
    log(&cli, &body);
    match cli.cmd {
        Cmd::Write { out } => fs::write(&out, &body).unwrap(),
    }
}
"#,
        )],
    );
    let written = |id: &str| places(&g, id).into_iter().map(|(_, _, artifact, _)| artifact).collect::<Vec<_>>();
    assert_eq!(written("options.main"), ["out/graph.json", "out/run.log", "out/run.log"]);
    assert_eq!(written("options.log"), [""], "a parameter holds what its callers hand");
    assert_eq!(written("options.grammar"), [""], "pest's `Parser` is no command line");
}

/// An entry's domain is what the human hands it on the command line: the binary's `main` takes the
/// options of the type clap parses its command line into, in the order of its fields, each as the
/// human types it. `#[arg(long)]` and `#[arg(short, long)]` name `--kebab-name`, `long = ".."` its
/// own name, `short` alone `-x`; a field with neither is a positional `<name>`; a flattened field
/// adds its type's options; a subcommand and a skipped field are no option. `Cli::parse()` and
/// `Cli::try_parse()` read the program's command line and are marked so; `Cli::parse_from(..)`
/// parses what it is handed and is not.
#[test]
fn clap_options_are_the_entrys_domain() {
    let g = written(
        "usage",
        &[(
            "src/main.rs",
            r#"use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
struct Cli {
    #[arg(short, long)]
    out_dir: PathBuf,
    #[arg(long = "dry-run")]
    dry: bool,
    #[arg(short)]
    verbose: bool,
    name: String,
    #[command(flatten)]
    common: Common,
    #[command(subcommand)]
    cmd: Option<Cmd>,
    #[arg(skip)]
    seen: u8,
}

#[derive(Args)]
struct Common {
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[derive(Subcommand)]
enum Cmd {
    Clean {
        #[arg(long)]
        all: bool,
    },
}

fn run(cli: &Cli) {}

fn tried() -> Result<(), clap::Error> {
    let cli = Cli::try_parse()?;
    run(&cli);
    Ok(())
}

fn given() {
    let cli = Cli::parse_from(["usage", "x"]);
    run(&cli);
}

fn main() {
    let cli = Cli::parse();
    run(&cli);
}
"#,
        )],
    );
    let options = ["--out-dir", "--dry-run", "-v", "<name>", "--log-level"].map(String::from).to_vec();
    let main = g.node("usage.main").unwrap();
    assert_eq!(main.entry.as_ref().map(|e| (e.kind, e.options.clone())), Some((EntryKind::Cli, options.clone())));
    let parse = main.sites.iter().find(|s| s.to == "usage.Cli").expect("main builds its Cli");
    assert_eq!(parse.command_line.as_ref(), Some(&options));
    let reads = |f: &str| g.node(f).unwrap().sites.iter().any(|s| s.command_line.is_some());
    assert!(reads("usage.tried") && !reads("usage.given"), "`try_parse` reads the command line, `parse_from` what it is handed");
    assert!(
        main.sites.iter().find(|s| s.to == "usage.run").unwrap().hands.iter().any(|h| h.name == "cli"),
        "what the parse gives is handed on"
    );
}

/// A generic type's arguments are values only where it holds or stands for them: std's and a
/// crate's containers (`IndexMap<String, Sheet>` holds Sheets), an `Option`, what an iterator
/// gives. Any other generic is its own name, a library's (`Area<Rgb, Shift>` is an `Area`, on
/// whatever colour and coordinates) and the program's own (`Layer<Sheet>` is a `Layer`) alike, in
/// a signature and in a struct's field. A callback (`impl Fn(&Sheet) -> Rgb`) is handed no value.
#[test]
fn a_generic_is_its_values_only_where_it_holds_them() {
    let g = written(
        "generic",
        &[(
            "src/lib.rs",
            r#"use canvas::{Area, Shift};
use indexmap::IndexMap;

pub struct Sheet {
    pub n: u8,
}

pub struct Rgb;

/// Values of one kind, stacked: `T` is the kind.
pub struct Layer<T> {
    items: Vec<T>,
}

pub struct Shelf {
    sheets: Vec<Sheet>,
    layer: Layer<Rgb>,
    area: Area<Rgb, Shift>,
}

pub fn kept(sheets: IndexMap<String, Sheet>, first: Option<Box<Sheet>>) -> impl Iterator<Item = Sheet> {
    sheets.into_values()
}

pub fn draw(area: &Area<Rgb, Shift>) {}

pub fn stack(layer: &Layer<Sheet>) -> Layer<Rgb> {
    Layer { items: vec![] }
}

pub fn each(paint: impl Fn(&Sheet) -> Rgb) {}
"#,
        )],
    );
    let of = |kind: EdgeKind, f: &str| {
        let mut out: Vec<String> = g.edges.iter().filter(|e| e.kind == kind && e.src == f).map(|e| e.dst.clone()).collect();
        out.sort();
        out
    };
    assert_eq!(of(EdgeKind::Takes, "generic.kept"), ["ext:indexmap.IndexMap", "generic.Sheet"]);
    assert_eq!(of(EdgeKind::Returns, "generic.kept"), ["generic.Sheet"]);
    assert_eq!(of(EdgeKind::Takes, "generic.draw"), ["ext:canvas.Area"]);
    assert_eq!(of(EdgeKind::Takes, "generic.stack"), ["generic.Layer"]);
    assert_eq!(of(EdgeKind::Returns, "generic.stack"), ["generic.Layer"]);
    assert_eq!(of(EdgeKind::Takes, "generic.each"), Vec::<String>::new());
    assert_eq!(of(EdgeKind::Has, "generic.Shelf"), ["generic.Layer", "generic.Sheet"]);
}

/// Crates written beside one another, the program read from `app` with each of `options`, and
/// removed.
fn crates(name: &str, files: &[(String, String)], options: &[Options]) -> Vec<Graph> {
    let dir = std::env::temp_dir().join(format!("atlas-rust-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (path, text) in files {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    let gs: Vec<_> = options.iter().map(|o| analyze(&dir.join("app"), o)).collect();
    let _ = std::fs::remove_dir_all(&dir);
    gs.into_iter().map(|g| g.expect("the crate parses")).collect()
}

/// Files as (path, text), each path below `at`.
fn below(at: &str, files: &[(&str, &str)]) -> Vec<(String, String)> {
    files.iter().map(|(p, t)| (format!("{at}{p}"), t.to_string())).collect()
}

/// A crate the program depends on, as Cargo keeps it: its root re-exports what its module `read`
/// defines, which reads the file it is handed.
const FILEKIT: &[(&str, &str)] = &[
    ("Cargo.toml", "[package]\nname = \"filekit\"\nversion = \"1.2.3\"\nedition = \"2021\"\n"),
    ("src/lib.rs", "mod clean;\nmod read;\n\npub use read::read_config;\n\npub fn unused() {}\n"),
    ("src/read.rs", "use std::fs;\n\nuse crate::clean::clean;\n\npub fn read_config(path: &str) -> String {\n    clean(fs::read_to_string(path).unwrap_or_default())\n}\n"),
    ("src/clean.rs", "pub fn clean(text: String) -> String {\n    text.trim().to_string()\n}\n"),
];

/// A crate the program depends on is read as code as far as the program's calls go into it: the
/// modules its path goes through (`filekit::read_config`, re-exported from `filekit::read`). Its
/// function is a step on the program's route, and what it reaches of the world is what the program
/// reaches through it: no table names the crate, yet the call reads the file the program hands it.
/// Each round of `--libraries` reaches one call further into the same crate; `--libraries 0`
/// reads none. Not read: a crate the tables describe, and one with compiled code (it links a
/// native library). What the program does not reach is no part of the picture, and `atlas check`
/// does not judge a library's code.
#[test]
fn a_crate_is_read_as_far_as_the_program_calls_into_it() {
    let mut files = below(
        "",
        &[
            (
                "app/Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nfilekit = { path = \"../filekit\" }\nfastsum = { path = \"../fastsum\" }\nwalkdir = { path = \"../walkdir\" }\n",
            ),
            ("app/src/main.rs", "use filekit::read_config;\n\nfn main() {\n    let text = read_config(\"settings.toml\");\n    fastsum::sum(&[1, 2]);\n    walkdir::WalkDir::new(\"data\");\n    println!(\"{text}\");\n}\n"),
            ("fastsum/Cargo.toml", "[package]\nname = \"fastsum\"\nversion = \"0.1.0\"\nedition = \"2021\"\nlinks = \"fastsum\"\n"),
            ("fastsum/src/lib.rs", "pub fn sum(xs: &[u32]) -> u32 {\n    xs.iter().sum()\n}\n"),
            ("walkdir/Cargo.toml", "[package]\nname = \"walkdir\"\nversion = \"2.0.0\"\nedition = \"2021\"\n"),
            ("walkdir/src/lib.rs", "pub struct WalkDir;\n\nimpl WalkDir {\n    pub fn new(root: &str) -> WalkDir {\n        WalkDir\n    }\n}\n"),
        ],
    );
    files.extend(below("filekit/", FILEKIT));
    let rounds = |n: usize| Options { libraries: n, ..Options::default() };
    let [none, one, two]: [Graph; 3] = crates("libraries", &files, &[rounds(0), rounds(1), rounds(2)]).try_into().ok().unwrap();
    let read = one.node("filekit.read.read_config").expect("the function the program calls is read");
    assert!(read.library, "a library's function is marked as one");
    assert!(one.parts.iter().all(|p| !p.id.starts_with("filekit")), "a library is no part of the program");
    assert!(
        places(&one, "filekit.read.read_config").contains(&place("disk_read", "read_to_string", "settings.toml", &["app.main"])),
        "{:?}",
        places(&one, "filekit.read.read_config")
    );
    let steps = |g: &Graph| g.node("app.main").unwrap().sites.iter().map(|s| s.to.clone()).filter(|t| !t.is_empty()).collect::<Vec<_>>();
    assert_eq!(steps(&one), ["filekit.read.read_config"]);
    assert!(one.node("filekit.unused").is_none(), "what the program does not reach is no part of the picture");
    assert!(one.node("filekit.clean.clean").is_none() && two.node("filekit.clean.clean").is_some(), "each round reaches one call further");
    for g in [&one, &two] {
        assert!(
            g.nodes.iter().all(|n| !n.id.starts_with("fastsum") && !n.id.starts_with("walkdir")),
            "neither compiled code nor a crate the tables describe is read"
        );
        assert!(atlas_engine::principles::check(g).iter().all(|f| !f.node.starts_with("filekit")), "a library's code is not judged");
    }
    assert!(none.nodes.iter().all(|n| !n.library) && steps(&none).is_empty(), "no round reads no crate");
}

/// A crate fetched from a registry is read where Cargo keeps it (`CARGO_HOME`), at the version
/// the lock names; nothing is fetched, and a crate not on disk stays a library's name.
#[test]
fn a_crate_is_read_where_cargo_keeps_it() {
    let lock = "version = 4\n\n[[package]]\nname = \"app\"\nversion = \"0.1.0\"\ndependencies = [\n \"absent\",\n \"filekit\",\n]\n\n[[package]]\nname = \"absent\"\nversion = \"0.3.0\"\nsource = \"registry+https://example.org/index\"\n\n[[package]]\nname = \"filekit\"\nversion = \"1.2.3\"\nsource = \"registry+https://example.org/index\"\n";
    let mut files = below(
        "",
        &[
            ("app/Cargo.toml", "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nfilekit = \"1.2\"\nabsent = \"0.3\"\n"),
            ("app/Cargo.lock", lock),
            ("app/src/main.rs", "fn main() {\n    let text = filekit::read_config(\"settings.toml\");\n    absent::run(&text);\n}\n"),
        ],
    );
    let registry = "cargo/registry/src/index.example.org-0000000000000000/";
    files.extend(below(&format!("{registry}filekit-1.2.3/"), FILEKIT));
    // another version beside it, which the lock does not name
    files.extend(below(
        &format!("{registry}filekit-1.0.0/"),
        &[
            ("Cargo.toml", "[package]\nname = \"filekit\"\nversion = \"1.0.0\"\n"),
            ("src/lib.rs", "pub fn read_config(path: &str) -> String {\n    String::new()\n}\n"),
        ],
    ));
    let home = std::env::temp_dir().join(format!("atlas-rust-registry-{}", std::process::id())).join("cargo");
    std::env::set_var("CARGO_HOME", &home);
    let g = crates("registry", &files, &[Options::default()]).remove(0);
    std::env::remove_var("CARGO_HOME");
    let read = g.node("filekit.read.read_config").expect("the version the lock names is read");
    assert!(read.file.contains("filekit-1.2.3"), "{}", read.file);
    assert!(g.node("filekit.read_config").is_none(), "not another version");
    assert!(g.nodes.iter().all(|n| !n.id.starts_with("absent")), "a crate not on disk is not read");
    assert!(g.node("app.main").unwrap().sites.iter().any(|s| s.to == "filekit.read.read_config"));
}

/// What lies beyond the rounds is glued into the crate's function at the border: the program
/// calls `save`, whose file is written two calls deeper, in functions the graph does not hold.
/// `save` is one arrow that writes the file the program hands it, drawn at its own call that leads
/// there, so the program's route reaches the place. Where the rounds reach the function that
/// writes, the touch is that function's own, not `save`'s.
#[test]
fn what_lies_beyond_the_rounds_is_the_border_functions() {
    let files = below(
        "",
        &[
            ("app/Cargo.toml", "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ndeepkit = { path = \"../deepkit\" }\n"),
            ("app/src/main.rs", "fn main() {\n    deepkit::save(\"out.txt\", \"rows\");\n}\n"),
            ("deepkit/Cargo.toml", "[package]\nname = \"deepkit\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
            (
                "deepkit/src/lib.rs",
                "use std::fs;\n\npub fn save(path: &str, text: &str) {\n    prepare(path, text);\n}\n\nfn prepare(path: &str, text: &str) {\n    write(path, text.trim());\n}\n\nfn write(path: &str, text: &str) {\n    fs::write(path, text).unwrap();\n}\n",
            ),
        ],
    );
    let rounds = |n: usize| Options { libraries: n, ..Options::default() };
    let [one, three]: [Graph; 2] = crates("beyond", &files, &[rounds(1), rounds(3)]).try_into().ok().unwrap();
    assert!(one.node("deepkit.prepare").is_none() && one.node("deepkit.write").is_none(), "what lies beyond the rounds is not drawn");
    let save = one.node("deepkit.save").expect("the function the program calls");
    assert!(
        places(&one, "deepkit.save").contains(&place("disk_write", "write", "out.txt", &["app.main"])),
        "the border writes what its code beyond writes, at the path the program hands it: {:?}",
        save.touches
    );
    let call = save.sites.iter().find(|s| s.to == "deepkit.prepare").expect("save's call into what lies beyond");
    assert!(save.touches.iter().all(|t| t.line == call.line), "drawn at the border's call that leads there");
    let route = atlas_engine::story::route(&one, "app.main", 2).unwrap();
    assert!(route.contains("out.txt"), "the program's route reaches the place:\n{route}");
    assert!(places(&three, "deepkit.write").iter().any(|(kind, ..)| kind == "disk_write"));
    assert!(places(&three, "deepkit.save").iter().all(|(kind, ..)| kind != "disk_write"), "a function's own arrow is not what it reaches");
}

/// A value read through `&`, `&mut`, parentheses, `?` or `.await` is the value inside it: a `let`
/// that holds `&mut Square::new()` holds a Square, as one that holds `&Square::new()` does.
#[test]
fn a_value_through_its_wrappers_is_the_value_inside() {
    let g = written(
        "wrappers",
        &[(
            "src/lib.rs",
            "pub struct Square;\n\nimpl Square {\n    pub fn new() -> Square { Square }\n    pub fn area(&self) -> f64 { 1.0 }\n}\n\npub fn both() -> f64 {\n    let s = &mut Square::new();\n    let t = &Square::new();\n    let u = (Square::new());\n    s.area() + t.area() + u.area()\n}\n",
        )],
    );
    let both = &g.node("wrappers.both").unwrap().sites;
    let areas = both.iter().filter(|s| s.to == "wrappers.Square.area").count();
    assert_eq!(areas, 3, "{both:#?}");
}

/// What a call gives is kept by the name a `let .. else` binds of it (`let Some(n) = self.node(id) else { .. }`), as an
/// `if let` keeps it; and a method called on a part of `self` among a macro's tokens (`format!(.., self.kind.name())`)
/// runs on a projection of the instance, as written outside a macro, so it is handed the instance (#159).
#[test]
fn a_let_else_keeps_what_it_binds_and_a_macro_s_method_on_self_is_handed_the_instance() {
    let g = written(
        "kept",
        &[(
            "src/lib.rs",
            "pub enum Kind { A, B }\n\nimpl Kind {\n    pub fn name(&self) -> &str {\n        match self { Kind::A => \"a\", Kind::B => \"b\" }\n    }\n}\n\npub struct Touch { pub kind: Kind, pub what: String }\n\nimpl Touch {\n    pub fn verb(&self) -> String {\n        format!(\"queries {}\", self.kind.name())\n    }\n}\n\npub struct Index { pub touches: Vec<Touch> }\n\nimpl Index {\n    fn node(&self, at: usize) -> Option<&Touch> {\n        self.touches.get(at)\n    }\n\n    pub fn label(&self, at: usize) -> String {\n        let Some(n) = self.node(at) else { return String::new() };\n        n.verb()\n    }\n}\n",
        )],
    );
    let label = g.node("kept.Index.label").expect("label is read");
    let node = label.sites.iter().find(|s| s.to == "kept.Index.node").expect("label calls node");
    assert_eq!(node.gives.iter().map(|h| (h.ty.as_str(), h.name.as_str())).collect::<Vec<_>>(), [("kept.Touch", "n")]);
    let verb = g.node("kept.Touch.verb").expect("verb is read");
    let name = verb.sites.iter().find(|s| s.to == "kept.Kind.name").expect("verb calls name");
    assert_eq!(name.hands.iter().map(|h| (h.ty.as_str(), h.name.as_str())).collect::<Vec<_>>(), [("kept.Touch", "")]);
    assert_eq!(name.hands[0].fills, [atlas_engine::Slot::Receiver]);
}
